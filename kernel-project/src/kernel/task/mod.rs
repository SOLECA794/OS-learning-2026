//! 任务与调度（W3 最小：RV 单核 + 用户态时钟抢占 RR）。
//!
//! 模型（docs/03 §3 子集）：
//! - `TaskControlBlock` 持有用户地址空间（`Box<MemorySet>`，切任务时 activate 其
//!   根页表）、内核栈（`KernelStack`，生命周期所有者）与内核态切换现场
//!   （`TaskContext`）。用户态现场（`TrapContext`）位于内核栈顶下方，由恢复汇编
//!   用 sp 定位，不另存指针。
//! - 调度：全局 FIFO 就绪队列；时钟中断 → 当前任务入队尾 → `__switch` 切下一个。
//!   内核态不抢占：trap 进入时 sstatus.SIE 被硬件清零，handler 期间无嵌套中断，
//!   只有用户态（SIE=1）会被时钟打断——符合规划书「先用户态抢占、内核暂不抢占」。
//!
//! 并发前提（M2 登记，见 log/boundary/M2-scheduler-rv.md）：仅 bootstrap hart
//! 运行内核（从核在 kernel_main park），就绪队列与 TCB 无跨核竞争；M3 SMP 落地时
//! 必须为就绪队列引入锁并审计中断屏蔽。

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::arch;
use crate::kernel::mm::address::{AddressRange, VirtAddr};
use crate::kernel::mm::kernel_stack::KernelStack;
use crate::kernel::mm::memory_set::MemorySet;

/// 最小自旋锁。M2 单核无竞争；M3 SMP 前需审计中断屏蔽与锁顺序。
pub(crate) struct SpinLock<T> {
    locked: core::sync::atomic::AtomicBool,
    data: core::cell::UnsafeCell<T>,
}

// SAFETY: M2 仅 bootstrap hart 访问；M3 SMP 落地时锁本身提供互斥。
unsafe impl<T: Send> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            locked: core::sync::atomic::AtomicBool::new(false),
            data: core::cell::UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        while self
            .locked
            .compare_exchange_weak(
                false,
                true,
                core::sync::atomic::Ordering::Acquire,
                core::sync::atomic::Ordering::Relaxed,
            )
            .is_err()
        {
            core::hint::spin_loop();
        }
        SpinLockGuard { lock: self }
    }
}

pub(crate) struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> core::ops::Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: 持有锁期间独占访问。
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> core::ops::DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: 持有锁期间独占访问。
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, core::sync::atomic::Ordering::Release);
    }
}

/// 任务控制块（W3 最小字段集）。
pub(crate) struct TaskControlBlock {
    pub id: usize,
    /// 用户地址空间（含内核身份映射；切任务时 activate 本空间根页表）。
    pub space: Box<MemorySet>,
    /// 内核栈（生命周期所有者；TrapContext 位于栈顶下方）。
    pub kstack: KernelStack,
    /// 内核态切换现场（`__switch` 保存/恢复）。
    pub task_ctx: arch::task::TaskContext,
}

/// 全局调度状态。并发前提见模块头。
static SCHED: SpinLock<Scheduler> = SpinLock::new(Scheduler::new());

struct Scheduler {
    /// 就绪任务 id 队列（FIFO：队尾入队、队首出队）。
    ready: VecDeque<usize>,
    /// id -> 任务槽。Box 保证 TCB 地址稳定：`__switch` 的现场指针跨 Vec 重排安全。
    tasks: Vec<Option<Box<TaskControlBlock>>>,
    /// 当前运行任务 id。
    current: usize,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            ready: VecDeque::new(),
            tasks: Vec::new(),
            current: 0,
        }
    }

    /// 登记一个新任务并返回分配到的 id。
    fn push(&mut self, mut task: TaskControlBlock) -> usize {
        let id = self.tasks.len();
        task.id = id;
        self.tasks.push(Some(Box::new(task)));
        id
    }
}

/// 用户栈顶固定低于内核身份映射区（RV: 内核 0x8000_0000 起）。
const USER_STACK_TOP: usize = 0x7f00_0000;

/// 从 ELF 构建一个用户任务：独立地址空间 + 内核栈 + 栈顶 TrapContext + 首次切换现场。
pub(crate) fn create_task(elf: &crate::kernel::elf::Elf, uapp: &[u8]) -> Result<TaskControlBlock, ()> {
    let layout = crate::kernel::mm::kernel_space::KernelImageLayout::current();
    let memory_bytes = crate::kernel::mm::configured_memory_bytes();
    let (_, memory_end) = arch::mm::physical_memory_range(memory_bytes)
        .expect("configured memory range unavailable");

    let user_stack = AddressRange::new(
        VirtAddr::new(USER_STACK_TOP - 0x1000),
        VirtAddr::new(USER_STACK_TOP),
    )
    .expect("user stack range");

    let mut space = MemorySet::new_user(layout, VirtAddr::new(memory_end), &elf.segments, user_stack)
        .map_err(|_| ())?;
    space.load_elf(&elf.segments, uapp).map_err(|_| ())?;

    let kstack = KernelStack::new().map_err(|_| ())?;
    let kstack_top = kstack.top();
    let frame_size = core::mem::size_of::<arch::trap::TrapContext>();

    // 栈顶下方放用户态现场；首次被调度时 __restore 用 sp 定位它。
    let context = arch::trap::TrapContext::new_user(elf.entry, USER_STACK_TOP);
    let context_ptr = (kstack_top - frame_size) as *mut arch::trap::TrapContext;
    // SAFETY: context_ptr 位于新分配内核栈顶下方，无重叠使用。
    unsafe { context_ptr.write(context) };

    // 首次切换现场：ra = trap 返回入口，sp = 现场基址。
    // 该任务被时钟抢占后，__switch 会用真实现场覆盖这两项。
    let task_ctx = arch::task::TaskContext {
        ra: arch::task::trap_return_address(),
        sp: kstack_top - frame_size,
        ..arch::task::TaskContext::zero_init()
    };

    Ok(TaskControlBlock {
        id: 0, // 由 Scheduler::push 重新分配
        space: Box::new(space),
        kstack,
        task_ctx,
    })
}

/// 登记 T0 为首任务（current，不进就绪队列），返回其根页表 PPN 与 TrapContext 指针。
pub(crate) fn launch_first(task: TaskControlBlock) -> (usize, *const arch::trap::TrapContext) {
    let mut sched = SCHED.lock();
    let id = sched.push(task);
    let ppn = sched.tasks[id].as_ref().unwrap().space.root_ppn().value();
    let ctx = {
        let t = sched.tasks[id].as_ref().unwrap();
        let frame_size = core::mem::size_of::<arch::trap::TrapContext>();
        (t.kstack.top() - frame_size) as *const arch::trap::TrapContext
    };
    sched.current = id;
    (ppn, ctx)
}

/// 把一个任务放入就绪队列（队尾）。
pub(crate) fn push_ready(task: TaskControlBlock) {
    let mut sched = SCHED.lock();
    let id = sched.push(task);
    sched.ready.push_back(id);
}

/// 时钟中断触发的调度点：当前任务入队尾，切换下一个任务。
///
/// 切回时（本任务重新获得 CPU）从 `__switch` 返回，恢复自己的页表后继续；
/// 返回后由 trap 恢复路径 sret 回用户态。
pub(crate) fn schedule() {
    // 锁内只做队列/状态更新并取出切换所需指针，随后立即释放锁再 __switch。
    // 关键：__switch 会挂起当前执行流（切到另一任务），若持锁切走，对方再进
    // schedule 会在 SpinLock 上自旋死锁（W3 实测根因）。
    let (old_ptr, new_ptr, new_ppn, from_id, to_id) = {
        let mut sched = SCHED.lock();
        let current = sched.current;
        let next = {
            sched.ready.push_back(current);
            sched.ready.pop_front().expect("ready queue empty after re-enqueue")
        };
        if next == current {
            return;
        }
        // 两槽互异且均被占用，Box 地址稳定（Vec 重排不影响）。
        let old_ptr: *mut arch::task::TaskContext = {
            let t = sched.tasks[current].as_mut().expect("current slot occupied");
            &mut t.task_ctx
        };
        let new_ptr: *mut arch::task::TaskContext = {
            let t = sched.tasks[next].as_mut().expect("next slot occupied");
            &mut t.task_ctx
        };
        let new_ppn = sched.tasks[next].as_ref().unwrap().space.root_ppn().value();
        sched.current = next;
        (old_ptr, new_ptr, new_ppn, current, next)
    }; // 锁在此处释放（guard drop）

    crate::log_info!("[sched] task {} -> task {}", from_id, to_id);

    // 先切页表再切现场：两份页表都含内核身份映射，切换后继续执行安全。
    // SAFETY: new_ppn 来自就绪任务的有效根页表。
    unsafe { arch::mm::activate(new_ppn) };
    arch::task::switch(old_ptr, new_ptr);
    // 切回本任务（重新获得 CPU）：恢复自己的页表（当前 satp 仍是对方页表）。
    let my_ppn = {
        let sched = SCHED.lock();
        sched.tasks[sched.current].as_ref().unwrap().space.root_ppn().value()
    };
    unsafe { arch::mm::activate(my_ppn) };
}

/// RV 启动双任务（M2）：定时器先行，T0 直进用户态，T1 就绪等待时钟抢占。
pub(crate) fn boot_tasks() -> ! {
    crate::arch::timer::init();
    #[cfg(feature = "uapp-fault-test")]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp_fault.elf");
    #[cfg(not(feature = "uapp-fault-test"))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp.elf");
    let uapp: &[u8] = UAPP_ELF;
    let elf = crate::kernel::elf::parse(uapp).expect("uapp ELF parse failed");

    let t0 = create_task(&elf, uapp).expect("task0 create failed");
    let t1 = create_task(&elf, uapp).expect("task1 create failed");
    let (ppn, ctx) = launch_first(t0);
    push_ready(t1);

    crate::log_info!("M2: two tasks ready (ids 0/1), timer armed, entering user mode");
    // SAFETY: ppn 是 T0 的有效根页表（launch_first 返回）。
    unsafe { arch::mm::activate(ppn) };
    arch::trap::restore_from(ctx)
}

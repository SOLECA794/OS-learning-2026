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

pub(crate) mod fd_table;

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
        self.lock
            .locked
            .store(false, core::sync::atomic::Ordering::Release);
    }
}

/// 任务生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskState {
    /// 就绪：在就绪队列中等待调度。
    Ready,
    /// 当前占用 CPU（不在就绪队列）。
    Running,
    /// 等待子任务退出（不在就绪队列；子 exit 时唤醒）。
    WaitingChild,
    /// 等待控制台输入（不在就绪队列；UART RX 时唤醒）。
    WaitingConsole,
    /// 已退出（僵尸，等待父回收；不在就绪队列）。
    Exited,
}

/// 任务控制块。
pub(crate) struct TaskControlBlock {
    pub id: usize,
    /// 父任务 id；boot 首任务为 None。
    pub parent: Option<usize>,
    /// 生命周期状态（驱动 schedule 的入队/回收决策）。
    pub state: TaskState,
    /// 退出码（Linux 语义：低 8 位经 wait status 编码返回）。
    pub exit_code: i32,
    /// 用户地址空间（含内核身份映射；切任务时 activate 本空间根页表）。
    pub space: Box<MemorySet>,
    /// 内核栈（生命周期所有者；TrapContext 位于栈顶下方）。
    pub kstack: KernelStack,
    /// 内核态切换现场（`__switch` 保存/恢复）。
    pub task_ctx: arch::task::TaskContext,
    /// 进程 FD 表（V2 §9：open/dup/close 的唯一入口；fork 整表复制共享偏移）。
    pub fds: fd_table::FdTable,
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
pub(crate) fn create_task(
    elf: &crate::kernel::elf::Elf,
    uapp: &[u8],
) -> Result<TaskControlBlock, ()> {
    let layout = crate::kernel::mm::kernel_space::KernelImageLayout::current();
    let memory_bytes = crate::kernel::mm::configured_memory_bytes();
    let (_, memory_end) =
        arch::mm::physical_memory_range(memory_bytes).expect("configured memory range unavailable");

    let user_stack = AddressRange::new(
        VirtAddr::new(USER_STACK_TOP - 0x1000),
        VirtAddr::new(USER_STACK_TOP),
    )
    .expect("user stack range");

    let mut space =
        MemorySet::new_user(layout, VirtAddr::new(memory_end), &elf.segments, user_stack)
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
        parent: None,
        state: TaskState::Running, // boot 首任务直进用户态
        exit_code: 0,
        space: Box::new(space),
        kstack,
        task_ctx,
        fds: fd_table::FdTable::new(),
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
        // Ready/Running 任务重新入队（Running 被抢占后回 Ready 等下次调度）；
        // Waiting/Exited 不再调度。
        let state = sched.tasks[current].as_ref().unwrap().state;
        if state == TaskState::Ready || state == TaskState::Running {
            sched.tasks[current].as_mut().unwrap().state = TaskState::Ready;
            sched.ready.push_back(current);
        }
        let next = match sched.ready.pop_front() {
            Some(next) => next,
            None => {
                let awaiting_console = sched
                    .tasks
                    .iter()
                    .flatten()
                    .any(|t| t.state == TaskState::WaitingConsole);
                drop(sched);
                if !awaiting_console {
                    crate::log_info!("[sched] ready queue empty, parking");
                    crate::arch::park();
                }
                crate::log_info!("[sched] idle waiting for console input");
                // 当前任务内核栈充当临时 idle 栈；被 IRQ 唤醒后仍从此处继续。
                // 没有并发退出/回收该任务：单 hart，SIE 仅在 idle_once 中打开。
                loop {
                    if !SCHED.lock().ready.is_empty() {
                        break;
                    }
                    arch::interrupt::idle_once();
                }
                sched = SCHED.lock();
                sched
                    .ready
                    .pop_front()
                    .expect("idle woke without a ready task")
            }
        };
        if next == current {
            sched.tasks[current].as_mut().unwrap().state = TaskState::Running;
            return;
        }
        sched.tasks[next].as_mut().unwrap().state = TaskState::Running;
        // 两槽互异且均被占用，Box 地址稳定（Vec 重排不影响）。
        let old_ptr: *mut arch::task::TaskContext = {
            let t = sched.tasks[current]
                .as_mut()
                .expect("current slot occupied");
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
        sched.tasks[sched.current]
            .as_ref()
            .unwrap()
            .space
            .root_ppn()
            .value()
    };
    unsafe { arch::mm::activate(my_ppn) };
}

/// exec：把当前任务的地址空间一次性替换为新空间（G4）。
///
/// 调用方必须先 activate(new_space.root_ppn()) 再调用本函数——旧空间被 drop
/// （释放页表树与用户页）时硬件必须已切换，避免释放仍被 satp 引用的页表。
pub(crate) fn exec_replace_space(new_space: MemorySet) {
    let mut sched = SCHED.lock();
    let current = sched.current;
    sched.tasks[current]
        .as_mut()
        .expect("current task missing during exec")
        .space = Box::new(new_space);
}
/// RV 启动单任务（M3）：T0 直进用户态，由 uapp 自行 fork 建立父子关系。
pub(crate) fn boot_tasks(hartid: usize) -> ! {
    crate::arch::timer::init();
    crate::arch::interrupt::init(hartid);
    crate::fs::init();
    #[cfg(feature = "uapp-fault-test")]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp_fault.elf");
    // G5 shell 演示模式（make run-shell）：task0 = shell.elf，跑固定输入脚本 /cmds.txt。
    #[cfg(all(
        feature = "uapp-shell",
        not(feature = "uapp-fault-test"),
        not(feature = "uapp-cow"),
        not(feature = "uapp-vfs"),
        not(feature = "uapp-console")
    ))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/shell.elf");
    // G6 COW 验收模式（make run-cow）：task0 = uapp-cow.elf，输出 C1–C5 断言。
    #[cfg(all(
        feature = "uapp-cow",
        not(feature = "uapp-shell"),
        not(feature = "uapp-fault-test"),
        not(feature = "uapp-vfs"),
        not(feature = "uapp-console")
    ))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp-cow.elf");
    #[cfg(all(
        feature = "uapp-console",
        not(feature = "uapp-vfs"),
        not(feature = "uapp-shell"),
        not(feature = "uapp-cow"),
        not(feature = "uapp-fault-test")
    ))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp-console.elf");
    #[cfg(all(feature = "uapp-vfs", not(feature = "uapp-fault-test")))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp-vfs.elf");
    #[cfg(all(
        not(feature = "uapp-shell"),
        not(feature = "uapp-fault-test"),
        not(feature = "uapp-cow"),
        not(feature = "uapp-vfs"),
        not(feature = "uapp-console")
    ))]
    const UAPP_ELF: &[u8] = include_bytes!("../../../build/user/uapp.elf");
    let uapp: &[u8] = UAPP_ELF;
    let elf = crate::kernel::elf::parse(uapp).expect("uapp ELF parse failed");

    let t0 = create_task(&elf, uapp).expect("task0 create failed");
    #[cfg(feature = "m28-frame-fault-test")]
    crate::kernel::mm::memory_set::test_cow_allocation_failures();
    let (ppn, ctx) = launch_first(t0);
    #[cfg(feature = "m28-frame-fault-test")]
    test_fork_current_allocation_failures();

    crate::log_info!("M3: task 0 ready, timer armed, entering user mode (uapp will fork)");
    // SAFETY: ppn 是 T0 的有效根页表（launch_first 返回）。
    unsafe { arch::mm::activate(ppn) };
    arch::trap::restore_from(ctx)
}

/// 以显式闭包访问当前任务的 FD 表（V2 §3：fs/syscall 不读取 Task 私有字段）。
/// 闭包内不得再取 SCHED 锁（锁已在持有中）。
pub(crate) fn with_current_fds<R>(f: impl FnOnce(&mut fd_table::FdTable) -> R) -> R {
    let mut sched = SCHED.lock();
    let current = sched.current;
    let t = sched.tasks[current].as_mut().expect("current task missing");
    f(&mut t.fds)
}

/// 当前任务 id（日志/诊断用）。
pub(crate) fn current_pid() -> usize {
    let sched = SCHED.lock();
    sched.current
}

/// 持 SCHED 锁调用闭包访问当前任务地址空间（trap 缺页处理用）。
/// 任务槽不存在时返回 None。
pub(crate) fn with_current_space<R>(
    f: impl FnOnce(&mut crate::kernel::mm::memory_set::MemorySet) -> R,
) -> Option<R> {
    let mut sched = SCHED.lock();
    let current = sched.current;
    sched.tasks[current].as_mut().map(|t| f(&mut t.space))
}
/// wait4 单步检查结果。
pub(crate) enum WaitResult {
    /// 子已退出的临时快照；资源仍由任务槽持有，写回成功后才能回收。
    Exited { pid: usize, exit_code: i32 },
    /// 存在未退出的子任务（调用方可选择阻塞等待）。
    WouldBlock,
    /// 没有匹配的子任务（Linux ECHILD）。
    NoChild,
}

/// fork：写时复制（COW）当前任务地址空间并复制现场，登记子任务并放入就绪队列。
///
/// 子进程从 fork 返回 0（现场 a0 改写、sepc 不变 → 从 fork 返回处继续）；父进程返回子 id。
/// V2 G6：可写用户页经 `MemorySet::dup_cow` 变为双侧只读共享，写时触发缺页复制。
pub(crate) fn fork_current() -> Result<usize, isize> {
    const ENOMEM: isize = -12;
    let layout = crate::kernel::mm::kernel_space::KernelImageLayout::current();
    let memory_bytes = crate::kernel::mm::configured_memory_bytes();
    let (_, memory_end) = arch::mm::physical_memory_range(memory_bytes).expect("memory range");

    let mut sched = SCHED.lock();
    let current = sched.current;

    // 先完成可能失败的内核栈分配，避免失败的 fork 已经改变父页权限。
    let kstack = KernelStack::new().map_err(|_| ENOMEM)?;
    // COW 复制地址空间：父可写页同步改为只读共享（需 &mut 父 space）。
    let new_space = {
        let parent = sched.tasks[current].as_mut().unwrap();
        parent
            .space
            .dup_cow(layout, VirtAddr::new(memory_end))
            .map_err(|_| ENOMEM)?
    };

    // 新内核栈 + 子现场（父现场副本，a0=0）。
    let (kstack, task_ctx) = {
        let parent = sched.tasks[current].as_ref().unwrap();
        let kstack_top = kstack.top();
        let frame_size = core::mem::size_of::<arch::trap::TrapContext>();
        let parent_ctx_ptr = (parent.kstack.top() - frame_size) as *const arch::trap::TrapContext;
        // SAFETY: 父现场位于其内核栈顶下方，trap 处理期间有效。
        let mut child_ctx = unsafe { *parent_ctx_ptr };
        child_ctx.set_syscall_return(0);
        let context_ptr = (kstack_top - frame_size) as *mut arch::trap::TrapContext;
        // SAFETY: 新内核栈顶下方，本函数独占。
        unsafe { context_ptr.write(child_ctx) };
        let task_ctx = arch::task::TaskContext {
            ra: arch::task::trap_return_address(),
            sp: kstack_top - frame_size,
            ..arch::task::TaskContext::zero_init()
        };
        (kstack, task_ctx)
    };

    // 复制 FD 表：File 项 Arc clone → 父子共享同一打开文件描述（共享偏移）。
    let child_fds = {
        let parent = sched.tasks[current].as_ref().unwrap();
        parent.fds.clone()
    };
    let child_tcb = TaskControlBlock {
        id: 0,
        parent: Some(current),
        state: TaskState::Ready,
        exit_code: 0,
        space: Box::new(new_space),
        kstack,
        task_ctx,
        fds: child_fds,
    };
    let child_id = sched.push(child_tcb);
    sched.ready.push_back(child_id);
    crate::log_info!(
        "[fork] task {} forked task {}  (frames={} wm={})",
        current,
        child_id,
        crate::kernel::mm::frame_allocator::allocated_count(),
        crate::kernel::mm::frame_allocator::bump_watermark()
    );
    Ok(child_id)
}

/// 测试完整 fork 路径的早期故障：第 0 点是连续内核栈，第 1 点起为子页表。
/// 只在启动首任务之后、恢复用户态之前执行，因此任务槽和等待队列均应保持原样。
#[cfg(feature = "m28-frame-fault-test")]
fn test_fork_current_allocation_failures() {
    use crate::kernel::mm::{address::VirtAddr, frame_allocator};
    let baseline_frames = frame_allocator::allocated_count();
    let baseline_refs = frame_allocator::test_shared_ref_entries();
    let stack_vpn = VirtAddr::new(USER_STACK_TOP - 0x1000).floor();
    assert!(!with_current_space(|space| space.cow_contains(stack_vpn)).unwrap());
    for budget in 0..9 {
        frame_allocator::set_test_failure_budget(Some(budget));
        assert_eq!(
            fork_current(),
            Err(-12),
            "fork succeeded at point {}",
            budget
        );
        assert_eq!(frame_allocator::test_allocation_attempts(), budget + 1);
        assert_eq!(frame_allocator::allocated_count(), baseline_frames);
        assert_eq!(frame_allocator::test_shared_ref_entries(), baseline_refs);
        assert!(!with_current_space(|space| space.cow_contains(stack_vpn)).unwrap());
        let sched = SCHED.lock();
        assert_eq!(sched.tasks.len(), 1, "failed fork published child task");
        assert!(sched.ready.is_empty(), "failed fork published ready child");
    }
    crate::log_info!("M28-FORK-OOM-PASS points=9");
}

/// exit：标记 Exited 并存储退出码；唤醒等待本任务的父；调度切走，永不返回。
///
/// 资源不立即释放（僵尸语义）：父 wait4 回收时 drop space/kstack。
pub(crate) fn exit_current(code: i32) -> ! {
    let current = {
        let mut sched = SCHED.lock();
        let current = sched.current;
        {
            let t = sched.tasks[current].as_mut().unwrap();
            t.state = TaskState::Exited;
            t.exit_code = code;
        }
        // 唤醒等待本任务退出的父（保守唤醒：父 wait 重查后再决定阻塞与否）。
        let parent = sched.tasks[current].as_ref().unwrap().parent;
        if let Some(pid) = parent {
            if let Some(p) = sched.tasks[pid].as_mut() {
                if p.state == TaskState::WaitingChild {
                    p.state = TaskState::Ready;
                    sched.ready.push_back(pid);
                }
            }
        }
        current
    };
    crate::log_info!("[exit] task {} exited with code {}", current, code);
    schedule(); // Exited 不入队，切走；队列空则 park。
    unreachable!("exited task resumed")
}

/// 只观察退出状态，不消费僵尸；必须扫描所有匹配任务，不能被运行中的子任务挡住。
pub(crate) fn wait4_step(pid: isize) -> WaitResult {
    let sched = SCHED.lock();
    let current = sched.current;
    let mut has_child = false;
    for (child_id, tcb) in sched
        .tasks
        .iter()
        .enumerate()
        .filter_map(|(id, t)| t.as_ref().map(|t| (id, t)))
    {
        if tcb.parent == Some(current) && (pid < 0 || child_id as isize == pid) {
            has_child = true;
            if tcb.state == TaskState::Exited {
                return WaitResult::Exited {
                    pid: child_id,
                    exit_code: tcb.exit_code,
                };
            }
        }
    }
    if has_child {
        WaitResult::WouldBlock
    } else {
        WaitResult::NoChild
    }
}

/// 状态写回成功后提交回收。当前单核、内核不可抢占，观察与提交之间不调度，
/// 也不存在同进程的另一个等待者；引入 SMP/线程后必须重新设计等待认领协议。
pub(crate) fn reap_child(pid: usize) {
    let mut sched = SCHED.lock();
    let child = sched.tasks[pid].as_ref().expect("wait child disappeared");
    assert!(child.parent == Some(sched.current) && child.state == TaskState::Exited);
    let exit_code = child.exit_code;
    // 任务模块是回收的唯一 owner；释放地址空间、内核栈以及 FD 引用。
    sched.tasks[pid] = None;
    crate::log_info!(
        "[wait] reaped task {} code {}  (frames={} wm={})",
        pid,
        exit_code,
        crate::kernel::mm::frame_allocator::allocated_count(),
        crate::kernel::mm::frame_allocator::bump_watermark()
    );
}

/// 当前任务阻塞等待子任务：状态 WaitingChild（不入就绪队列）后调度切走；
/// 被唤醒后从 schedule 返回，调用方应重新执行 wait 检查。
pub(crate) fn block_current_and_wait() {
    {
        let mut sched = SCHED.lock();
        let current = sched.current;
        let t = sched.tasks[current].as_mut().unwrap();
        t.state = TaskState::WaitingChild;
    }
    schedule();
}

/// 在 SIE=0 的 syscall 上下文中，原子完成「确认无数据 → 登记等待」。
/// IRQ 只能在返回用户态或 idle_once 打开中断后进入，因此不会丢唤醒。
pub(crate) fn block_current_on_console_if_empty() {
    let current = {
        let mut sched = SCHED.lock();
        if arch::console::has_input() {
            return;
        }
        let current = sched.current;
        sched.tasks[current].as_mut().unwrap().state = TaskState::WaitingConsole;
        current
    };
    crate::log_info!("[console] task {} waiting for input", current);
    schedule();
}

/// UART IRQ 在字节进入 RX 队列后调用；只改变 task 状态，不消费字节。
/// 多个读者可一起唤醒并重新竞争字节，未拿到数据者会再次等待。
pub(crate) fn wake_console_readers() {
    let mut sched = SCHED.lock();
    for id in 0..sched.tasks.len() {
        if let Some(t) = sched.tasks[id].as_mut() {
            if t.state == TaskState::WaitingConsole {
                t.state = TaskState::Ready;
                sched.ready.push_back(id);
            }
        }
    }
}

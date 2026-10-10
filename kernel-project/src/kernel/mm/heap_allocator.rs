//! 可释放、可合并的内核全局堆。

use core::alloc::{GlobalAlloc, Layout};
use core::mem::{align_of, size_of};
use core::ptr::null_mut;

use crate::klib::sync::SpinLock;

use super::address::PAGE_SIZE;
use super::frame_allocator::{self, FrameAllocError, FrameBlock};

/// M1 固定预留 1 MiB。移除条件：实现可按页扩展的内核虚拟堆区。
const HEAP_PAGES: usize = 256;
const HEAP_BYTES: usize = HEAP_PAGES * PAGE_SIZE;
const ALLOCATION_MAGIC: usize = 0x4b48_4541_5041_4c4c;

/// 内核堆初始化失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapInitError {
    /// 全局堆已经持有 backing，禁止重复替换。
    AlreadyInitialized,
    /// 无法从页帧分配器取得堆 backing。
    Frame(FrameAllocError),
}

impl From<FrameAllocError> for HeapInitError {
    fn from(value: FrameAllocError) -> Self {
        Self::Frame(value)
    }
}

/// 内核堆当前状态的只读快照，仅用于诊断。
#[derive(Debug, Clone, Copy)]
pub struct HeapStats {
    /// backing 总容量，单位为字节。
    pub total_bytes: usize,
    /// 所有空闲链表节点的容量之和。
    pub free_bytes: usize,
    /// 尚未释放的 allocation 数量。
    pub allocations: usize,
}

/// 存放在空闲内存自身起始位置的链表节点。
#[repr(C)]
struct FreeNode {
    /// 当前空闲区域的总字节数，包含该节点自身。
    size: usize,
    /// 按地址递增排列的下一个空闲区域。
    next: *mut FreeNode,
}

/// 位于用户指针之前的内部元数据。
///
/// 它记录包含对齐填充和被吸收尾部碎片的真实范围，使释放时能够完整归还空间。
#[repr(C)]
struct AllocationHeader {
    /// 用于尽早发现错误指针或重复释放的固定标记。
    magic: usize,
    /// 本次真实占用块的起始地址，即 header 地址。
    block_start: usize,
    /// 本次真实占用的字节数。
    block_size: usize,
}

/// 受全局自旋锁保护的堆状态。
struct HeapState {
    /// 不位于 backing 内的哨兵节点，简化链表头部插入和删除。
    head: FreeNode,
    /// 堆所占连续页的唯一所有权；内核运行期间不会整体释放。
    backing: Option<FrameBlock>,
    /// backing 包含的第一个内核虚拟地址。
    start: usize,
    /// backing 不包含的结束内核虚拟地址。
    end: usize,
    /// 当前真实占用字节数，包含 allocation header 与对齐填充。
    allocated_bytes: usize,
    /// 当前存活 allocation 数量。
    allocations: usize,
}

// SAFETY: 所有裸指针只指向 backing 内存，并且只能在 HEAP_STATE 锁内访问。
unsafe impl Send for HeapState {}

impl HeapState {
    /// 创建尚未绑定 backing 的静态初始状态。
    const fn new() -> Self {
        Self {
            head: FreeNode {
                size: 0,
                next: null_mut(),
            },
            backing: None,
            start: 0,
            end: 0,
            allocated_bytes: 0,
            allocations: 0,
        }
    }

    /// 接管页帧块并把整个范围发布为第一个空闲区域。
    fn initialize(&mut self, backing: FrameBlock) -> Result<(), HeapInitError> {
        if self.backing.is_some() {
            return Err(HeapInitError::AlreadyInitialized);
        }
        let start = crate::arch::mm::phys_to_virt(backing.start().start_address().value());
        let size = backing
            .pages()
            .checked_mul(PAGE_SIZE)
            .expect("kernel heap size overflow");
        self.start = start;
        self.end = start.checked_add(size).expect("kernel heap end overflow");
        self.backing = Some(backing);
        // SAFETY: backing 独占、已清零且覆盖完整堆范围；链表目前为空。
        unsafe { self.insert_free(start, size) };
        Ok(())
    }

    /// 使用 first-fit 策略寻找满足 `Layout` 的空闲区域。
    fn allocate(&mut self, layout: Layout) -> *mut u8 {
        if self.backing.is_none() {
            return null_mut();
        }
        let user_size = layout.size().max(1);
        let user_align = layout.align().max(align_of::<AllocationHeader>());
        let header_size = size_of::<AllocationHeader>();
        let minimum_free = size_of::<FreeNode>();
        let mut previous: *mut FreeNode = &mut self.head;

        // SAFETY: previous/current 只遍历锁保护下、位于 backing 中的有效空闲链表。
        unsafe {
            while !(*previous).next.is_null() {
                let region = (*previous).next;
                let region_start = region as usize;
                let Some(region_end) = region_start.checked_add((*region).size) else {
                    panic!("kernel heap free-list range overflow");
                };
                let Some(header_end) = region_start.checked_add(header_size) else {
                    return null_mut();
                };
                let Some(mut user_start) = align_up_checked(header_end, user_align) else {
                    return null_mut();
                };
                let mut block_start = user_start - header_size;

                // 不能留下装不下 FreeNode 的前缀；把用户地址推进到下一对齐位置。
                if block_start != region_start && block_start - region_start < minimum_free {
                    let Some(minimum_user) = region_start
                        .checked_add(minimum_free)
                        .and_then(|value| value.checked_add(header_size))
                    else {
                        return null_mut();
                    };
                    let Some(next_user) = align_up_checked(minimum_user, user_align) else {
                        return null_mut();
                    };
                    user_start = next_user;
                    block_start = user_start - header_size;
                }

                let Some(user_end) = user_start.checked_add(user_size) else {
                    return null_mut();
                };
                let Some(mut block_end) = align_up_checked(user_end, align_of::<FreeNode>()) else {
                    return null_mut();
                };
                if block_end > region_end {
                    previous = region;
                    continue;
                }
                // 尾部碎片不足以成为节点时纳入本次 allocation，由 header 记录真实范围。
                if block_end != region_end && region_end - block_end < minimum_free {
                    block_end = region_end;
                }

                (*previous).next = (*region).next;
                let prefix = block_start - region_start;
                let suffix = region_end - block_end;
                if prefix != 0 {
                    self.insert_free(region_start, prefix);
                }
                if suffix != 0 {
                    self.insert_free(block_end, suffix);
                }

                let header = block_start as *mut AllocationHeader;
                header.write(AllocationHeader {
                    magic: ALLOCATION_MAGIC,
                    block_start,
                    block_size: block_end - block_start,
                });
                self.allocated_bytes += block_end - block_start;
                self.allocations += 1;
                return user_start as *mut u8;
            }
        }
        null_mut()
    }

    /// 读取用户指针前的 header，并将其真实占用范围放回空闲链表。
    ///
    /// # Safety
    /// `pointer` 必须由本分配器返回、尚未释放，且对应对象已经结束生命周期。
    unsafe fn deallocate(&mut self, pointer: *mut u8) {
        let header_address = (pointer as usize)
            .checked_sub(size_of::<AllocationHeader>())
            .expect("kernel heap allocation header underflow");
        let header = header_address as *mut AllocationHeader;
        // SAFETY: GlobalAlloc 契约要求 pointer 来自本分配器且尚未释放。
        let allocation = unsafe { header.read() };
        assert_eq!(
            allocation.magic, ALLOCATION_MAGIC,
            "invalid kernel heap free"
        );
        let allocation_end = allocation
            .block_start
            .checked_add(allocation.block_size)
            .expect("kernel heap allocation range overflow");
        assert!(
            allocation.block_start >= self.start && allocation_end <= self.end,
            "kernel heap free outside backing range"
        );
        assert!(self.allocations > 0 && self.allocated_bytes >= allocation.block_size);
        self.allocations -= 1;
        self.allocated_bytes -= allocation.block_size;
        // SAFETY: header 证明该范围是一个完整、尚未回收的 allocation。
        unsafe { self.insert_free(allocation.block_start, allocation.block_size) };
    }

    /// 按地址顺序插入空闲区域，并与前后相邻区域合并。
    ///
    /// # Safety
    /// `[address, address + size)` 必须位于 backing 中，且不得与现有空闲区域重叠。
    unsafe fn insert_free(&mut self, address: usize, size: usize) {
        assert_eq!(address & (align_of::<FreeNode>() - 1), 0);
        assert!(size >= size_of::<FreeNode>());
        let end = address
            .checked_add(size)
            .expect("kernel heap free range overflow");
        let mut previous: *mut FreeNode = &mut self.head;

        // SAFETY: 链表按地址递增，节点均位于 backing 的空闲区域。
        unsafe {
            while !(*previous).next.is_null() && ((*previous).next as usize) < address {
                previous = (*previous).next;
            }
            let next = (*previous).next;
            if previous != &mut self.head as *mut FreeNode {
                let previous_end = (previous as usize)
                    .checked_add((*previous).size)
                    .expect("kernel heap previous free range overflow");
                assert!(previous_end <= address, "heap free overlap");
            }
            assert!(next.is_null() || end <= next as usize, "heap free overlap");

            let node = address as *mut FreeNode;
            node.write(FreeNode { size, next });
            (*previous).next = node;

            if !(*node).next.is_null() && end == (*node).next as usize {
                let following = (*node).next;
                (*node).size += (*following).size;
                (*node).next = (*following).next;
            }
            if previous != &mut self.head as *mut FreeNode
                && (previous as usize)
                    .checked_add((*previous).size)
                    .is_some_and(|previous_end| previous_end == node as usize)
            {
                (*previous).size += (*node).size;
                (*previous).next = (*node).next;
            }
        }
    }

    /// 在锁内遍历空闲链表并生成诊断快照。
    fn stats(&self) -> HeapStats {
        let mut free_bytes = 0;
        let mut current = self.head.next;
        // SAFETY: 只在锁内读取有效链表。
        unsafe {
            while !current.is_null() {
                free_bytes += (*current).size;
                current = (*current).next;
            }
        }
        HeapStats {
            total_bytes: self.end.saturating_sub(self.start),
            free_bytes,
            allocations: self.allocations,
        }
    }
}

/// 为 Rust `alloc` API 提供同步全局分配能力。
pub struct KernelHeap {
    state: SpinLock<HeapState>,
}

impl KernelHeap {
    /// 构造尚未初始化的全局分配器对象，供静态变量使用。
    const fn new() -> Self {
        Self {
            state: SpinLock::new(HeapState::new()),
        }
    }
}

// SAFETY: HeapState 的所有访问均由 SpinLock 串行化，返回块互不重叠。
unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.state.lock().allocate(layout)
    }

    unsafe fn dealloc(&self, pointer: *mut u8, _layout: Layout) {
        // SAFETY: GlobalAlloc 调用方保证 pointer/layout 对应一次未释放分配。
        unsafe { self.state.lock().deallocate(pointer) }
    }
}

#[global_allocator]
/// Rust `Box`、`Vec`、`Arc` 等类型使用的唯一内核全局分配器。
static HEAP_ALLOCATOR: KernelHeap = KernelHeap::new();

/// 从页帧分配器预留固定容量并初始化全局内核堆。
pub fn init() -> Result<(), HeapInitError> {
    let backing = frame_allocator::allocate_contiguous(HEAP_PAGES)?;
    assert_eq!(backing.pages() * PAGE_SIZE, HEAP_BYTES);
    HEAP_ALLOCATOR.state.lock().initialize(backing)
}

/// 获取锁保护下的一致堆状态快照。
pub fn stats() -> HeapStats {
    HEAP_ALLOCATOR.state.lock().stats()
}

/// 启动期最小自检：覆盖不同对齐、增长、释放与相邻空闲块合并。
pub fn self_test() {
    use alloc::boxed::Box;
    use alloc::vec::Vec;

    let free_before = stats().free_bytes;
    {
        let boxed = Box::new([0x5au8; 64]);
        assert!(boxed.iter().all(|&byte| byte == 0x5a));
        let mut values = Vec::with_capacity(32);
        for value in 0..32usize {
            values.push(value * 3);
        }
        assert_eq!(values[31], 93);
    }
    let after = stats();
    assert_eq!(after.allocations, 0, "kernel heap self-test leaked objects");
    assert_eq!(
        after.free_bytes, free_before,
        "kernel heap did not coalesce"
    );
}

/// 在不发生整数回绕的前提下向上对齐到二的幂边界。
fn align_up_checked(value: usize, align: usize) -> Option<usize> {
    debug_assert!(align.is_power_of_two());
    value
        .checked_add(align - 1)
        .map(|value| value & !(align - 1))
}

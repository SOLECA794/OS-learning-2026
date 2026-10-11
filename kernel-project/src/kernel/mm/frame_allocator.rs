//! 启动期物理页帧分配器。
//!
//! 未引入堆之前，已归还页直接把 free-list next 指针保存在页首。

use core::mem::ManuallyDrop;
use core::ptr;

use crate::klib::sync::SpinLock;

use super::address::{PhysPageNum, PAGE_SIZE};

/// 页帧分配器初始化和分配失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameAllocError {
    /// 在提供可用物理内存范围前调用了分配接口。
    NotInitialized,
    /// 同一个全局分配器被重复初始化。
    AlreadyInitialized,
    /// 初始化范围或连续分配请求不包含任何页面。
    EmptyRange,
    /// 剩余物理页不足以满足请求。
    OutOfMemory,
}

/// 全局页帧分配器的锁内状态。
///
/// `[start, next)` 已经被分发过，`[next, end)` 尚未使用；`recycled` 串联已归还的页。
struct FrameAllocator {
    /// 本分配器管理范围的起始物理页号。
    start: usize,
    /// bump 分配路径下一次返回的物理页号。
    next: usize,
    /// 管理范围不包含的结束物理页号。
    end: usize,
    /// 回收单链表的表头；链表 next 值直接存放在空闲页首部。
    recycled: Option<usize>,
    /// 当前仍由调用方持有的页数，仅用于资源统计和下溢检查。
    allocated: usize,
    /// 是否已经接收过有效物理内存范围。
    initialized: bool,
}

impl FrameAllocator {
    /// 构造尚未绑定物理内存的静态初始状态。
    const fn new() -> Self {
        Self {
            start: 0,
            next: 0,
            end: 0,
            recycled: None,
            allocated: 0,
            initialized: false,
        }
    }

    /// 安装唯一的可用物理页号半开区间 `[start, end)`。
    fn init(&mut self, start: PhysPageNum, end: PhysPageNum) -> Result<(), FrameAllocError> {
        if self.initialized {
            return Err(FrameAllocError::AlreadyInitialized);
        }
        if start >= end {
            return Err(FrameAllocError::EmptyRange);
        }
        self.start = start.value();
        self.next = start.value();
        self.end = end.value();
        self.initialized = true;
        Ok(())
    }

    /// 优先复用已归还页面，否则从尚未使用的范围顺序取得一页。
    fn allocate(&mut self) -> Result<PhysPageNum, FrameAllocError> {
        if !self.initialized {
            return Err(FrameAllocError::NotInitialized);
        }
        let ppn = if let Some(ppn) = self.recycled {
            let address = PhysPageNum::new(ppn).start_address().value();
            let pointer = crate::arch::mm::phys_to_virt(address) as *const usize;
            // SAFETY: recycled 指向由本分配器独占的空闲页，页首保存下一节点页号。
            let next = unsafe { pointer.read() };
            self.recycled = if next == usize::MAX { None } else { Some(next) };
            ppn
        } else {
            if self.next == self.end {
                return Err(FrameAllocError::OutOfMemory);
            }
            let ppn = self.next;
            self.next += 1;
            ppn
        };
        self.allocated += 1;
        Ok(PhysPageNum::new(ppn))
    }

    /// 从尚未使用的 bump 区分配连续页。
    ///
    /// 回收链表中的离散页不参与连续分配。保留原因：任务内核栈需要连续虚拟地址，
    /// 当前又尚无独立的内核虚拟地址分配器；移除条件是内核 vmalloc/栈映射器落地。
    fn allocate_contiguous(&mut self, pages: usize) -> Result<PhysPageNum, FrameAllocError> {
        if !self.initialized {
            return Err(FrameAllocError::NotInitialized);
        }
        if pages == 0 {
            return Err(FrameAllocError::EmptyRange);
        }
        let next = self
            .next
            .checked_add(pages)
            .filter(|&next| next <= self.end)
            .ok_or(FrameAllocError::OutOfMemory)?;
        let start = self.next;
        self.next = next;
        self.allocated += pages;
        Ok(PhysPageNum::new(start))
    }

    /// 将一个不再被引用的页面压入回收链表。
    ///
    /// # Safety
    /// 调用方必须拥有该页的唯一所有权，并保证它没有被页表或对象继续引用。
    unsafe fn deallocate(&mut self, ppn: PhysPageNum) {
        assert!(
            self.initialized,
            "physical frame allocator is not initialized"
        );
        assert!(
            ppn.value() >= self.start && ppn.value() < self.next,
            "physical frame does not belong to allocator"
        );
        assert!(
            self.allocated > 0,
            "physical frame allocation count underflow"
        );

        let address = crate::arch::mm::phys_to_virt(ppn.start_address().value()) as *mut usize;
        let next = self.recycled.unwrap_or(usize::MAX);
        // SAFETY: 调用方移交了该页的唯一所有权，页已不再用于数据或页表。
        unsafe { address.write(next) };
        self.recycled = Some(ppn.value());
        self.allocated -= 1;
    }
}

/// 串行化所有页帧状态转换，当前所有 CPU 共享同一个分配器。
static FRAME_ALLOCATOR: SpinLock<FrameAllocator> = SpinLock::new(FrameAllocator::new());

/// 初始化全局页帧分配器管理的物理页范围。
pub fn init(start: PhysPageNum, end: PhysPageNum) -> Result<(), FrameAllocError> {
    FRAME_ALLOCATOR.lock().init(start, end)
}

/// 分配并清零一个页面，返回其唯一所有权令牌。
pub fn allocate() -> Result<FrameTracker, FrameAllocError> {
    let ppn = FRAME_ALLOCATOR.lock().allocate()?;
    let address = crate::arch::mm::phys_to_virt(ppn.start_address().value()) as *mut u8;
    // SAFETY: 刚分配出的页由返回的 FrameTracker 独占，且范围正好是一页。
    unsafe { ptr::write_bytes(address, 0, PAGE_SIZE) };
    Ok(FrameTracker { ppn })
}

/// 分配并清零指定数量的连续页面。
pub fn allocate_contiguous(pages: usize) -> Result<FrameBlock, FrameAllocError> {
    let start = FRAME_ALLOCATOR.lock().allocate_contiguous(pages)?;
    let address = crate::arch::mm::phys_to_virt(start.start_address().value()) as *mut u8;
    let bytes = pages
        .checked_mul(PAGE_SIZE)
        .expect("physical frame block size overflow");
    // SAFETY: 分配器刚把连续范围的唯一所有权交给 FrameBlock，范围可由内核访问。
    unsafe { ptr::write_bytes(address, 0, bytes) };
    Ok(FrameBlock { start, pages })
}

/// 返回当前尚未归还的物理页总数，仅用于日志和诊断。
pub fn allocated_count() -> usize {
    FRAME_ALLOCATOR.lock().allocated
}

/// 不经过 RAII 令牌直接归还页面，供页表和映射区析构路径使用。
///
/// # Safety
/// 调用方必须证明该页不再被任何硬件页表或软件对象访问。
pub(super) unsafe fn deallocate(ppn: PhysPageNum) {
    // SAFETY: 由调用方保证该页不再被任何映射或页表引用。
    unsafe { FRAME_ALLOCATOR.lock().deallocate(ppn) }
}

/// 一个物理页帧的唯一所有权令牌。
pub struct FrameTracker {
    ppn: PhysPageNum,
}

impl FrameTracker {
    /// 查询令牌拥有的物理页号，不转移所有权。
    pub const fn ppn(&self) -> PhysPageNum {
        self.ppn
    }

    /// 把页所有权转交给页表或映射区。
    pub fn into_ppn(self) -> PhysPageNum {
        let this = ManuallyDrop::new(self);
        this.ppn
    }
}

impl Drop for FrameTracker {
    fn drop(&mut self) {
        // SAFETY: FrameTracker 是该页的唯一 owner，析构后不存在其它合法访问者。
        unsafe { deallocate(self.ppn) }
    }
}

/// 一段连续物理页的唯一所有权令牌。
pub struct FrameBlock {
    start: PhysPageNum,
    pages: usize,
}

impl FrameBlock {
    /// 返回连续范围包含的第一个物理页号。
    pub const fn start(&self) -> PhysPageNum {
        self.start
    }

    /// 返回连续范围包含的页面数量。
    pub const fn pages(&self) -> usize {
        self.pages
    }
}

impl Drop for FrameBlock {
    fn drop(&mut self) {
        for offset in 0..self.pages {
            let ppn = self
                .start
                .checked_add(offset)
                .expect("FrameBlock PPN overflow during cleanup");
            // SAFETY: FrameBlock 独占完整范围，且每页只在本循环中归还一次。
            unsafe { deallocate(ppn) };
        }
    }
}

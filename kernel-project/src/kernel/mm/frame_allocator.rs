//! 启动期物理页帧分配器。
//!
//! 未引入堆之前，空闲块的元数据（块大小、下一个块）直接保存在空闲块首页。
//! 空闲块链表按起始页号升序，分配连续页时从块尾切出，释放时按地址插入并与
//! 相邻块合并——连续区（内核栈、用户地址空间）在反复分配/释放后可复用，
//! 不依赖 bump 游标无限前进。

use alloc::collections::BTreeMap;
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
/// 空闲块链表：每个空闲块的首页保存 `[size][next]` 两个 usize（size=块内页数，
/// next=下一空闲块首页页号，`usize::MAX` 表示链表尾）；块按起始页号升序排列。
/// `[start, next)` 是已分发/可分发范围，`blocks` 是可复用的空闲区。
struct FrameAllocator {
    /// 本分配器管理范围的起始物理页号。
    start: usize,
    /// bump 分配路径下一次返回的物理页号。
    next: usize,
    /// 管理范围不包含的结束物理页号。
    end: usize,
    /// 空闲块链表头；空闲块首页同时保存块大小与后继指针。
    blocks: Option<usize>,
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
            blocks: None,
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

    /// 读取空闲块首页的元数据（size/next）。空闲页由分配器独占，读取安全。
    fn block_size(&self, ppn: usize) -> usize {
        let address = crate::arch::mm::phys_to_virt(PhysPageNum::new(ppn).start_address().value())
            as *const usize;
        // SAFETY: ppn 必须是当前链表上的空闲块首页，该页归分配器独占且未被分配出去。
        unsafe { address.read() }
    }

    fn block_next(&self, ppn: usize) -> Option<usize> {
        let address = crate::arch::mm::phys_to_virt(PhysPageNum::new(ppn).start_address().value())
            as *const usize;
        // SAFETY: 同上；next 字段位于首页偏移 8 字节处。
        let next = unsafe { address.add(1).read() };
        if next == usize::MAX {
            None
        } else {
            Some(next)
        }
    }

    /// 改写空闲块首页的 size（保留 next 不变）。
    fn set_block_size(&self, ppn: usize, size: usize) {
        let address = crate::arch::mm::phys_to_virt(PhysPageNum::new(ppn).start_address().value())
            as *mut usize;
        // SAFETY: ppn 必须是当前链表上的空闲块首页。
        unsafe {
            let next = address.add(1).read();
            address.write(size);
            address.add(1).write(next);
        }
    }

    /// 改写空闲块首页的 next（保留 size 不变）。
    fn set_block_next(&self, ppn: usize, next: Option<usize>) {
        let address = crate::arch::mm::phys_to_virt(PhysPageNum::new(ppn).start_address().value())
            as *mut usize;
        // SAFETY: ppn 必须是当前链表上的空闲块首页（或即将成为链表头的空闲块）。
        unsafe {
            let size = address.read();
            address.write(size);
            address.add(1).write(next.unwrap_or(usize::MAX));
        }
    }

    /// 把 `[ppn, ppn+pages)` 作为新空闲块写入元数据（链表顺序由调用方整理）。
    fn write_new_block(&mut self, ppn: usize, size: usize, next: Option<usize>) {
        let address = crate::arch::mm::phys_to_virt(PhysPageNum::new(ppn).start_address().value())
            as *mut usize;
        // SAFETY: 该段已由分配器收回独占，写入元数据合法；分配出去前会被清零覆盖。
        unsafe {
            address.write(size);
            address.add(1).write(next.unwrap_or(usize::MAX));
        }
    }

    /// 优先从空闲块切出 `pages` 页（从块尾切，保持块头与链表指针不变）。
    /// 返回切出段的起始页号；无满足块返回 None。
    fn allocate_from_blocks(&mut self, pages: usize) -> Option<usize> {
        let mut prev: Option<usize> = None;
        let mut cur = self.blocks;
        while let Some(b) = cur {
            let size = self.block_size(b);
            if size >= pages {
                let start = b + (size - pages);
                let remain = size - pages;
                if remain == 0 {
                    // 整块消费：摘除块头，prev 的 next 指向块的 next。
                    let next = self.block_next(b);
                    match prev {
                        Some(p) => self.set_block_next(p, next),
                        None => self.blocks = next,
                    }
                } else {
                    self.set_block_size(b, remain);
                }
                return Some(start);
            }
            prev = Some(b);
            cur = self.block_next(b);
        }
        None
    }

    /// 优先复用空闲块，否则从尚未使用的范围顺序取得一页。
    fn allocate(&mut self) -> Result<PhysPageNum, FrameAllocError> {
        if !self.initialized {
            return Err(FrameAllocError::NotInitialized);
        }
        let ppn = if let Some(ppn) = self.allocate_from_blocks(1) {
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

    /// 分配连续页，优先从空闲块切，否则从 bump 区顺序取得。
    fn allocate_contiguous(&mut self, pages: usize) -> Result<PhysPageNum, FrameAllocError> {
        if !self.initialized {
            return Err(FrameAllocError::NotInitialized);
        }
        if pages == 0 {
            return Err(FrameAllocError::EmptyRange);
        }
        let start = if let Some(start) = self.allocate_from_blocks(pages) {
            start
        } else {
            let next = self
                .next
                .checked_add(pages)
                .filter(|&next| next <= self.end)
                .ok_or(FrameAllocError::OutOfMemory)?;
            let start = self.next;
            self.next = next;
            start
        };
        self.allocated += pages;
        Ok(PhysPageNum::new(start))
    }

    /// 把一个不再被引用的连续段归还到空闲块链表（排序插入 + 相邻合并）。
    ///
    /// # Safety
    /// 调用方必须拥有该段全部页的唯一所有权，并保证它们没有被页表或对象继续引用。
    unsafe fn deallocate_range(&mut self, ppn: usize, pages: usize) {
        assert!(
            self.initialized,
            "physical frame allocator is not initialized"
        );
        assert!(
            ppn >= self.start && ppn + pages <= self.next,
            "physical frame does not belong to allocator"
        );
        assert!(
            self.allocated >= pages,
            "physical frame allocation count underflow"
        );
        self.allocated -= pages;

        // 找插入位置：`cur` 是第一个起始页号 >= 待插入段尾部的块；`prev` 是其前驱。
        let start = ppn;
        let mut size = pages;
        let mut prev: Option<usize> = None;
        let mut cur = self.blocks;
        while let Some(b) = cur {
            if b >= start + size {
                break;
            }
            assert!(
                b + self.block_size(b) <= start,
                "physical frame free overlap"
            );
            prev = Some(b);
            cur = self.block_next(b);
        }

        // 与后继相邻则吸收后继块（可连续吸收多个）。
        while let Some(b) = cur {
            if start + size != b {
                break;
            }
            size += self.block_size(b);
            cur = self.block_next(b);
        }

        // 与前一空闲块相邻则并入 prev，否则作为新块插入 prev 与 cur 之间。
        match prev {
            Some(p) if p + self.block_size(p) == start => {
                self.set_block_size(p, self.block_size(p) + size);
                self.set_block_next(p, cur);
            }
            _ => {
                match prev {
                    Some(p) => self.set_block_next(p, Some(start)),
                    None => self.blocks = Some(start),
                }
                self.write_new_block(start, size, cur);
            }
        }
    }
}

/// 串行化所有页帧状态转换，当前所有 CPU 共享同一个分配器。
static FRAME_ALLOCATOR: SpinLock<FrameAllocator> = SpinLock::new(FrameAllocator::new());

/// COW 共享页引用计数表：仅登记被 fork 共享的物理页（ppn → 引用数）。
/// 未登记页视为独占（refcount=1，释放时直接归还）。引用数归零才真正
/// deallocate，其余释放路径只减计数。
static FRAME_REFS: SpinLock<BTreeMap<usize, usize>> = SpinLock::new(BTreeMap::new());

/// M2.8 测试构建专用：在成功分配指定次数后，让下一次物理帧申请失败一次。
/// 只在单核启动自检期间启用；生产构建没有此状态或分支。
#[cfg(feature = "m28-frame-fault-test")]
static FAULT_INJECTION: SpinLock<(Option<usize>, usize)> = SpinLock::new((None, 0));

#[cfg(feature = "m28-frame-fault-test")]
fn reject_test_allocation() -> bool {
    let mut state = FAULT_INJECTION.lock();
    state.1 += 1;
    match state.0 {
        Some(0) => {
            state.0 = None;
            true
        }
        Some(left) => {
            state.0 = Some(left - 1);
            false
        }
        None => false,
    }
}

/// 仅供启动自检设置一次性故障；None 表示只统计尝试次数。
#[cfg(feature = "m28-frame-fault-test")]
pub(crate) fn set_test_failure_budget(budget: Option<usize>) {
    *FAULT_INJECTION.lock() = (budget, 0);
}

/// 返回本轮分配尝试次数，失败后 budget 必须已自动撤销。
#[cfg(feature = "m28-frame-fault-test")]
pub(crate) fn test_allocation_attempts() -> usize {
    let state = FAULT_INJECTION.lock();
    assert!(state.0.is_none(), "frame fault injection was not consumed");
    state.1
}

/// 仅自检使用：检查共享引用登记没有残留；不会参与分配行为。
#[cfg(feature = "m28-frame-fault-test")]
pub(crate) fn test_shared_ref_entries() -> usize {
    FRAME_REFS.lock().len()
}

/// 初始化全局页帧分配器管理的物理页范围。
pub fn init(start: PhysPageNum, end: PhysPageNum) -> Result<(), FrameAllocError> {
    FRAME_ALLOCATOR.lock().init(start, end)
}

/// 分配并清零一个页面，返回其唯一所有权令牌。
pub fn allocate() -> Result<FrameTracker, FrameAllocError> {
    #[cfg(feature = "m28-frame-fault-test")]
    if reject_test_allocation() {
        return Err(FrameAllocError::OutOfMemory);
    }
    let ppn = FRAME_ALLOCATOR.lock().allocate()?;
    let address = crate::arch::mm::phys_to_virt(ppn.start_address().value()) as *mut u8;
    // SAFETY: 刚分配出的页由返回的 FrameTracker 独占，且范围正好是一页。
    unsafe { ptr::write_bytes(address, 0, PAGE_SIZE) };
    Ok(FrameTracker { ppn })
}

/// 分配并清零指定数量的连续页面。
pub fn allocate_contiguous(pages: usize) -> Result<FrameBlock, FrameAllocError> {
    #[cfg(feature = "m28-frame-fault-test")]
    if reject_test_allocation() {
        return Err(FrameAllocError::OutOfMemory);
    }
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

/// 返回 bump 路径已分发的页数（`next - start`），仅用于日志和诊断。
///
/// 连续区可复用后，反复分配/释放不应让该水位持续增长；水位稳定是
/// "空闲块复用生效"的直接证据（旧实现每轮 fork 消耗 ~527 页）。
pub fn bump_watermark() -> usize {
    let lock = FRAME_ALLOCATOR.lock();
    lock.next - lock.start
}

/// 启动自检：相邻块回收必须合并，重新分配必须复用，重用页必须清零。
/// 在 mm::init 中只执行一次；失败即停在 M1 成功标记之前。
pub fn self_test() {
    let baseline = allocated_count();
    let block = allocate_contiguous(5).expect("frame self-test block");
    let (first, second) = block.split_at(2);
    let start = first.start();
    let watermark = bump_watermark();
    // 先归还后块再归还前块，验证链表的相邻合并。
    drop(second);
    drop(first);
    let merged = allocate_contiguous(5).expect("frame self-test merged block");
    assert_eq!(merged.start(), start, "frame self-test did not reuse range");
    assert_eq!(bump_watermark(), watermark, "frame self-test advanced bump");
    drop(merged);

    let page = allocate().expect("frame self-test page");
    let address = crate::arch::mm::phys_to_virt(page.ppn().start_address().value()) as *mut u8;
    // SAFETY: page 独占此物理页，写入首末字节不会跨页。
    unsafe {
        address.write_volatile(0xa5);
        address.add(PAGE_SIZE - 1).write_volatile(0x5a);
    }
    let ppn = page.ppn();
    drop(page);
    let reused = allocate().expect("frame self-test reused page");
    assert_eq!(reused.ppn(), ppn, "frame self-test page was not reused");
    // SAFETY: reused 再次独占该页；allocate 已承诺清零整页。
    unsafe {
        assert_eq!(address.read_volatile(), 0);
        assert_eq!(address.add(PAGE_SIZE - 1).read_volatile(), 0);
    }
    drop(reused);
    assert_eq!(allocated_count(), baseline, "frame self-test leaked pages");
    crate::log_info!("M3-FRAME-REUSE-PASS");
}

/// 不经过 RAII 令牌直接归还页面，供页表和映射区析构路径使用。
///
/// # Safety
/// 调用方必须证明该页不再被任何硬件页表或软件对象访问。
pub(super) unsafe fn deallocate(ppn: PhysPageNum) {
    // SAFETY: 由调用方保证该页不再被任何映射或页表引用。
    unsafe { FRAME_ALLOCATOR.lock().deallocate_range(ppn.value(), 1) }
}

/// 登记一次 COW 共享：某物理页被新增一个映射引用。
///
/// 未登记页首次进入共享时计数从 1（原独占者）跳到 2；此后每次 retain +1。
/// 调用方必须保证该页当前已被某映射持有，且本调用对应一个新的共享映射。
pub(crate) fn cow_retain(ppn: PhysPageNum) {
    let mut refs = FRAME_REFS.lock();
    *refs.entry(ppn.value()).or_insert(1) += 1;
}

/// COW 感知释放：释放调用方持有的一个映射引用。
///
/// - 登记页：计数 -1，归零则移除登记并真正归还给分配器；
/// - 未登记页（独占）：与 deallocate 等价，直接归还。
///
/// # Safety
/// 调用方必须证明自己确实持有一个指向该页的映射引用，且此后不再访问该映射。
pub(super) unsafe fn deallocate_cow(ppn: PhysPageNum) {
    let mut refs = FRAME_REFS.lock();
    match refs.get_mut(&ppn.value()) {
        Some(count) => {
            *count -= 1;
            if *count == 0 {
                refs.remove(&ppn.value());
                drop(refs);
                // SAFETY: 计数归零意味着最后一个映射引用释放，页不再可达。
                unsafe { deallocate(ppn) };
            }
        }
        None => {
            drop(refs);
            // SAFETY: 独占页，调用方持有唯一引用。
            unsafe { deallocate(ppn) };
        }
    }
}

/// 不经过 RAII 令牌直接归还一段连续页面，供内核栈等整块析构路径使用。
///
/// # Safety
/// 调用方必须证明该段不再被任何硬件页表或软件对象访问。
pub(super) unsafe fn deallocate_block(start: PhysPageNum, pages: usize) {
    // SAFETY: 由调用方保证该段不再被任何映射或页表引用。
    unsafe {
        FRAME_ALLOCATOR
            .lock()
            .deallocate_range(start.value(), pages)
    }
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

    /// 把同一独占范围拆成两个相邻令牌，供回收合并自检使用。
    fn split_at(self, first_pages: usize) -> (Self, Self) {
        assert!(first_pages > 0 && first_pages < self.pages);
        let block = ManuallyDrop::new(self);
        (
            Self {
                start: block.start,
                pages: first_pages,
            },
            Self {
                start: PhysPageNum::new(block.start.value() + first_pages),
                pages: block.pages - first_pages,
            },
        )
    }
}

impl Drop for FrameBlock {
    fn drop(&mut self) {
        // SAFETY: FrameBlock 独占完整范围，且整段只归还一次（由分配器内部
        // 按地址排序插入并合并，保证元数据与链表不变量）。
        unsafe { deallocate_block(self.start, self.pages) }
    }
}

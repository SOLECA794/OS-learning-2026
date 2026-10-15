//! 映射区与地址空间所有权。

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use super::address::{AddressError, AddressRange, PhysPageNum, VirtAddr, VirtPageNum, VpnRange};
use super::frame_allocator::{self, FrameAllocError};
use super::kernel_space::KernelImageLayout;
use super::page_table::{MapPermission, PageTable, PageTableError};
use crate::arch::MemoryType;

/// 无内核堆阶段的临时容量上限；满时返回 AreaLimit，绝不覆盖已有区域。
/// 移除条件：kalloc 落地后由可增长的 MapArea 容器接管。
const MAX_MAP_AREAS: usize = 64;

/// 地址空间构造、映射和拆除过程中可能返回的错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    /// 地址范围、对齐或算术错误。
    Address(AddressError),
    /// 数据页或页表页分配失败。
    Frame(FrameAllocError),
    /// 页表遍历或叶子项状态错误。
    PageTable(PageTableError),
    /// 新区域与已有区域至少共享一个虚拟页。
    Overlap,
    /// 临时固定容量的区域表已满。
    AreaLimit,
    /// 按起始地址删除区域时没有找到目标。
    AreaNotFound,
}

impl From<AddressError> for MemoryError {
    fn from(value: AddressError) -> Self {
        Self::Address(value)
    }
}

impl From<FrameAllocError> for MemoryError {
    fn from(value: FrameAllocError) -> Self {
        Self::Frame(value)
    }
}

impl From<PageTableError> for MemoryError {
    fn from(value: PageTableError) -> Self {
        Self::PageTable(value)
    }
}

/// 一个映射区获取物理页的方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapType {
    /// 虚拟页号与物理页号相同，数据页不归 MemorySet 所有。
    Identity,
    /// 每个虚拟页持有一个物理页引用；fork 后可与其它地址空间共享。
    Framed,
}

/// 地址空间内一段权限和内存属性一致的连续映射。
#[derive(Debug, Clone, Copy)]
pub struct MapArea {
    /// 该区域覆盖的虚拟页半开区间。
    range: VpnRange,
    /// 物理页来源以及叶子页的所有权策略。
    map_type: MapType,
    /// CPU 对该区域允许的读、写、执行和用户访问权限。
    permissions: MapPermission,
    /// 普通缓存内存或强序设备内存。
    memory_type: MemoryType,
}

impl MapArea {
    /// 构造普通内存区域，并把字节范围扩展到完整页面。
    pub fn new(
        range: AddressRange<VirtAddr>,
        map_type: MapType,
        permissions: MapPermission,
    ) -> Result<Self, MemoryError> {
        Ok(Self {
            range: VpnRange::covering(range)?,
            map_type,
            permissions,
            memory_type: MemoryType::Normal,
        })
    }

    /// 构造读写、身份映射且使用设备内存属性的 MMIO 区域。
    pub fn new_device(range: AddressRange<VirtAddr>) -> Result<Self, MemoryError> {
        Ok(Self {
            range: VpnRange::covering(range)?,
            map_type: MapType::Identity,
            permissions: MapPermission::READ | MapPermission::WRITE,
            memory_type: MemoryType::Device,
        })
    }

    /// 返回该区域覆盖的虚拟页范围。
    pub const fn range(&self) -> VpnRange {
        self.range
    }

    /// 逐页建立映射；中途失败时撤销本次已经完成的部分。
    fn map_into(&self, page_table: &mut PageTable) -> Result<(), MemoryError> {
        let mut vpn = self.range.start();
        while vpn < self.range.end() {
            let result = match self.map_type {
                MapType::Identity => page_table
                    .map(
                        vpn,
                        PhysPageNum::new(vpn.value()),
                        self.permissions,
                        self.memory_type,
                    )
                    .map_err(MemoryError::from),
                MapType::Framed => match frame_allocator::allocate() {
                    Ok(frame) => {
                        let result = page_table
                            .map(vpn, frame.ppn(), self.permissions, self.memory_type)
                            .map_err(MemoryError::from);
                        if result.is_ok() {
                            let _ = frame.into_ppn();
                        }
                        result
                    }
                    Err(error) => Err(error.into()),
                },
            };
            if let Err(error) = result {
                self.rollback(page_table, self.range.start(), vpn);
                return Err(error);
            }
            vpn = vpn.checked_add(1)?;
        }
        Ok(())
    }

    /// 回滚 `[start, end)` 内由一次失败插入创建的叶子项。
    fn rollback(&self, page_table: &mut PageTable, start: VirtPageNum, end: VirtPageNum) {
        let mut vpn = start;
        while vpn < end {
            if let Ok(ppn) = page_table.unmap(vpn) {
                if self.map_type == MapType::Framed {
                    // SAFETY: 该叶子映射刚被移除；COW 页走引用计数释放，独占页直接归还。
                    unsafe { frame_allocator::deallocate_cow(ppn) };
                }
            }
            vpn = vpn.checked_add(1).expect("rollback VPN overflow");
        }
    }

    /// 删除整个区域；即使遇到错误也继续清理其余页面，并返回首个错误。
    fn unmap_from(&self, page_table: &mut PageTable) -> Result<(), MemoryError> {
        let mut vpn = self.range.start();
        let mut first_error = None;
        while vpn < self.range.end() {
            match page_table.unmap(vpn) {
                Ok(ppn) if self.map_type == MapType::Framed => {
                    // SAFETY: 页表叶子已清除；COW 页按引用计数释放（可能仍有其它进程引用）。
                    unsafe { frame_allocator::deallocate_cow(ppn) };
                }
                Ok(_) => {}
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
            vpn = vpn.checked_add(1)?;
        }
        first_error.map_or(Ok(()), |error| Err(error.into()))
    }
}

/// 一个地址空间的页表和映射区域集合。
///
/// `MemorySet` 拥有页表树；其中 `Framed` 区域还拥有对应叶子数据页。
pub struct MemorySet {
    page_table: PageTable,
    areas: [Option<MapArea>; MAX_MAP_AREAS],
    /// 处于 COW 共享状态的用户虚拟页（写时复制：PTE 已清写位，集合与 PTE 同域维护）。
    cow_pages: BTreeSet<VirtPageNum>,
    /// 用户堆起始地址（页对齐），brk 扩展从此处向上增长。
    heap_base: VirtAddr,
    /// 当前堆结束地址（program break）；初始等于 heap_base。
    heap_brk: VirtAddr,
}

impl MemorySet {
    /// 创建只有空根页表、尚未包含任何映射的地址空间。
    pub fn new_bare() -> Result<Self, MemoryError> {
        Ok(Self {
            page_table: PageTable::new()?,
            areas: [None; MAX_MAP_AREAS],
            cow_pages: BTreeSet::new(),
            heap_base: VirtAddr::new(0),
            heap_brk: VirtAddr::new(0),
        })
    }

    /// 构造内核映像的身份映射集合，但不激活它。
    pub fn new_kernel(
        layout: KernelImageLayout,
        physical_memory_end: VirtAddr,
    ) -> Result<Self, MemoryError> {
        let mut memory_set = Self::new_bare()?;
        memory_set.insert_kernel_mapping(layout, physical_memory_end)?;
        Ok(memory_set)
    }

    /// 构造用户地址空间：内核映射 + 用户 LOAD 段（Framed+USER）+ 用户栈（Framed+USER）。
    /// 段内容需另行调用 `load_elf` 拷入；0 页不映射（NULL 解引用 → 缺页受控）。
    pub fn new_user(
        layout: KernelImageLayout,
        physical_memory_end: VirtAddr,
        segments: &[crate::kernel::elf::LoadSegment],
        user_stack: AddressRange<VirtAddr>,
    ) -> Result<Self, MemoryError> {
        let mut memory_set = Self::new_bare()?;
        memory_set.insert_kernel_mapping(layout, physical_memory_end)?;
        // 映射区间按页合并后再建区：rustc 小程序的 .text/.rodata 常共享同一页
        // （uapp 实测：LOAD1 end=0x1004c、LOAD2 start=0x1004c），逐段 insert 会 Overlap。
        // 权限取并集（宽松；M2 引入逐页 PTE 精确权限后收窄）。内容仍按原始段拷贝。
        for (start, end, perms) in merged_user_ranges(segments) {
            let range = AddressRange::new(start.start_address(), end.start_address())?;
            memory_set.insert(MapArea::new(range, MapType::Framed, perms)?)?;
        }
        memory_set.insert(MapArea::new(
            user_stack,
            MapType::Framed,
            MapPermission::READ | MapPermission::WRITE | MapPermission::USER,
        )?)?;
        // 用户堆：从 0x2000_0000 开始向上增长，初始为空（brk == heap_base）。
        // 避开 0x1000_0000（QEMU UART0 MMIO）和 PLIC（0x0c00_0000）。
        memory_set.heap_base = VirtAddr::new(0x2000_0000);
        memory_set.heap_brk = memory_set.heap_base;
        Ok(memory_set)
    }

    /// 把 ELF LOAD 段内容拷入已映射的用户页（filesz 内容 + memsz-filesz 清零）。
    pub fn load_elf(
        &mut self,
        segments: &[crate::kernel::elf::LoadSegment],
        elf_bytes: &[u8],
    ) -> Result<(), MemoryError> {
        for seg in segments {
            if seg.file_bytes == 0 {
                continue;
            }
            let seg_end = seg.vaddr + seg.mem_bytes;
            let mut vpn = VirtAddr::new(seg.vaddr).floor();
            let end_vpn = VirtAddr::new(seg_end).ceil()?;
            while vpn < end_vpn {
                let page_va = vpn.value() << super::address::PAGE_SHIFT;
                let copy_start = core::cmp::max(page_va, seg.vaddr);
                let copy_end = core::cmp::min(page_va + super::address::PAGE_SIZE, seg_end);
                if copy_start < copy_end {
                    let file_src = seg.file_offset + (copy_start - seg.vaddr);
                    let ppn = self
                        .translate(VirtAddr::new(page_va))
                        .ok_or(MemoryError::AreaNotFound)?;
                    let dst = (ppn.value() << super::address::PAGE_SHIFT) + (copy_start - page_va);
                    let file_end = seg.file_offset + seg.file_bytes;
                    // SAFETY: dst 是本 MemorySet 刚建立的用户物理页（identity 映射可写），
                    // 来源 elf_bytes 是内核映像内的只读数据，区间均已校验。
                    unsafe {
                        if file_src < file_end {
                            let n = core::cmp::min(copy_end - copy_start, file_end - file_src);
                            core::ptr::copy_nonoverlapping(
                                elf_bytes.as_ptr().add(file_src),
                                dst as *mut u8,
                                n,
                            );
                            if n < copy_end - copy_start {
                                core::ptr::write_bytes(
                                    (dst + n) as *mut u8,
                                    0,
                                    (copy_end - copy_start) - n,
                                );
                            }
                        } else {
                            core::ptr::write_bytes(dst as *mut u8, 0, copy_end - copy_start);
                        }
                    }
                }
                vpn = vpn.checked_add(1)?;
            }
        }
        Ok(())
    }

    /// 完整复制地址空间（fork 用）：Framed 区域逐页分配新物理页并拷贝内容，
    /// Identity 区域（内核映射等）跳过、按参数重建一次内核映射。
    ///
    /// 注意：不触发 TLB 操作，调用方需在激活后按需刷新。
    ///
    /// 旧的全量复制路径：逐页拷贝内容。COW 落地后由 `dup_cow` 取代，
    /// 保留作调试对照与未来非 COW fork 场景。
    #[allow(dead_code)]
    pub fn dup(
        &self,
        layout: KernelImageLayout,
        physical_memory_end: VirtAddr,
    ) -> Result<Self, MemoryError> {
        let mut new_set = Self::new_bare()?;
        new_set.insert_kernel_mapping(layout, physical_memory_end)?;
        for area in self.areas.iter().flatten() {
            if area.map_type != MapType::Framed {
                continue;
            }
            let new_area = MapArea {
                range: area.range,
                map_type: MapType::Framed,
                permissions: area.permissions,
                memory_type: area.memory_type,
            };
            new_set.insert(new_area)?;
            // 逐页拷贝内容：源/目标均为各自页表持有的物理页，内核 identity 映射可直访。
            let mut vpn = area.range.start();
            while vpn < area.range.end() {
                let src_ppn = self
                    .page_table
                    .translate(vpn)
                    .ok_or(MemoryError::AreaNotFound)?;
                let dst_ppn = new_set
                    .page_table
                    .translate(vpn)
                    .ok_or(MemoryError::AreaNotFound)?;
                let src = (src_ppn.value() << super::address::PAGE_SHIFT) as *const u8;
                let dst = (dst_ppn.value() << super::address::PAGE_SHIFT) as *mut u8;
                // SAFETY: 两物理页均已分配且由各自页表持有，区间 = PAGE_SIZE。
                unsafe {
                    core::ptr::copy_nonoverlapping(src, dst, super::address::PAGE_SIZE);
                }
                vpn = vpn.checked_add(1)?;
            }
        }
        Ok(new_set)
    }
    /// 写时复制 fork：复用页表叶子的物理页，双侧（含父）改为只读 + COW 登记。
    ///
    /// - 可写叶：父 PTE 清写位并登记 COW，子 PTE 同（共享物理页），引用计数 +1；
    /// - 已 COW 叶：子继续共享，引用计数 +1；
    /// - 只读/可执行叶：共享并增加引用计数，但不登记 COW（不可写）；
    /// - Identity 区域（内核映射）跳过，按参数重建。
    /// 需要 &mut self：父地址空间的 PTE 也要改为 COW 只读。
    pub fn dup_cow(
        &mut self,
        layout: KernelImageLayout,
        physical_memory_end: VirtAddr,
    ) -> Result<Self, MemoryError> {
        let mut new_set = Self::new_bare()?;
        new_set.insert_kernel_mapping(layout, physical_memory_end)?;
        for area in self.areas.iter().flatten() {
            if area.map_type != MapType::Framed {
                continue;
            }
            let slot = new_set
                .areas
                .iter()
                .position(Option::is_none)
                .ok_or(MemoryError::AreaLimit)?;
            let mut vpn = area.range.start();
            // 构造期间父空间保持原样；失败只撤销当前区间已完成的映射。
            // 已完成区域由 new_set 的 Drop 释放，所有叶子（包括只读页）都持有引用。
            let result = (|| -> Result<(), MemoryError> {
                while vpn < area.range.end() {
                    let (ppn, perms, memtype) = self
                        .page_table
                        .leaf_info(vpn)
                        .ok_or(MemoryError::AreaNotFound)?;
                    let was_cow = self.cow_pages.contains(&vpn);
                    let child_perms = if perms.contains(MapPermission::WRITE) || was_cow {
                        new_set.cow_pages.insert(vpn);
                        perms.without(MapPermission::WRITE)
                    } else {
                        perms
                    };
                    new_set
                        .page_table
                        .map(vpn, ppn, child_perms, memtype)
                        .map_err(MemoryError::from)?;
                    // 只有映射成功才获取新引用；map 失败时没有孤立的引用计数。
                    frame_allocator::cow_retain(ppn);
                    vpn = vpn.checked_add(1).expect("mapped VPN overflow");
                }
                Ok(())
            })();
            if let Err(error) = result {
                area.rollback(&mut new_set.page_table, area.range.start(), vpn);
                return Err(error);
            }
            new_set.areas[slot] = Some(*area);
        }
        // 所有可能返回 ENOMEM 的页表构造已完成，才提交父空间的只读权限。
        // 这些叶子刚刚验证存在，更新失败属于内部不变量损坏而非可恢复分配失败。
        for &vpn in &new_set.cow_pages {
            let (_, perms, memtype) = self
                .page_table
                .leaf_info(vpn)
                .expect("COW parent leaf missing");
            self.page_table
                .set_leaf_permissions(vpn, perms.without(MapPermission::WRITE), memtype)
                .expect("COW parent permission commit failed");
            self.cow_pages.insert(vpn);
        }
        crate::arch::mm::flush_tlb();
        Ok(new_set)
    }

    /// COW store fault 处理入口（trap 调用）：
    /// vpn 在 COW 集合中则分配新页、拷贝、解除 COW 并恢复写权限、释放本进程对旧页的引用。
    /// 返回 Ok(true) 表示已处理（恢复执行）；Ok(false) 表示非 COW 页（调用方走 fatal）。
    pub(crate) fn handle_cow_store(&mut self, vpn: VirtPageNum) -> Result<bool, MemoryError> {
        if !self.cow_pages.contains(&vpn) {
            return Ok(false);
        }
        let (ppn, perms, memtype) = self
            .page_table
            .leaf_info(vpn)
            .ok_or(MemoryError::AreaNotFound)?;
        let new_frame = frame_allocator::allocate().map_err(MemoryError::from)?;
        let dst_ppn = new_frame.ppn();
        let src = (ppn.value() << super::address::PAGE_SHIFT) as *const u8;
        let dst = (dst_ppn.value() << super::address::PAGE_SHIFT) as *mut u8;
        // SAFETY: 旧页仍由本进程映射持有；新页由新 FrameTracker 独占；区间各为一页。
        unsafe {
            core::ptr::copy_nonoverlapping(src, dst, super::address::PAGE_SIZE);
        }
        let writable = perms.union(MapPermission::WRITE);
        // 关键：必须同时把叶子指向新页并恢复写权限（仅改权限会让 PTE 指向
        // 随后被释放的旧页，见 run-cow 实测 illegal instruction）。
        self.page_table
            .set_leaf(vpn, dst_ppn, writable, memtype)
            .map_err(MemoryError::from)?;
        self.cow_pages.remove(&vpn);
        let _ = new_frame.into_ppn();
        // SAFETY: 写者已切换到新页，释放对旧共享页的一个引用。
        unsafe { frame_allocator::deallocate_cow(ppn) };
        // 本核 PTE 已变化，刷新对应地址的翻译缓存。
        crate::arch::mm::flush_tlb();
        crate::log_info!(
            "[cow] copy vpn={} old_ppn={} new_ppn={}",
            vpn.value(),
            ppn.value(),
            dst_ppn.value()
        );
        Ok(true)
    }

    /// 查询虚拟页是否处于 COW 共享状态（trap 分发用）。
    ///
    /// 默认构建不直接调用；仅在 `m28-frame-fault-test` 故障注入测试中使用。
    #[allow(dead_code)]
    pub(crate) fn cow_contains(&self, vpn: VirtPageNum) -> bool {
        self.cow_pages.contains(&vpn)
    }

    /// brk(2)：设置 program break。
    ///
    /// - `new_brk == 0`：返回当前 brk，不改变映射。
    /// - `new_brk > heap_brk`：向上扩展堆，逐页分配物理帧并映射 RW 用户页。
    /// - `new_brk < heap_brk`：向下收缩堆，逐页 unmap 并释放物理帧。
    /// - `new_brk < heap_base`：拒绝，返回当前 brk。
    ///
    /// 返回实际 brk 地址（成功后等于 new_brk 的页对齐值）。
    pub(crate) fn brk(&mut self, new_brk: usize) -> Result<usize, MemoryError> {
        if new_brk == 0 {
            return Ok(self.heap_brk.value());
        }
        let target = VirtAddr::new(new_brk);
        if target < self.heap_base {
            return Ok(self.heap_brk.value());
        }
        if target > self.heap_brk {
            // 向上扩展：逐页分配并映射。
            let start_vpn = self.heap_brk.ceil()?;
            let end_vpn = target.ceil()?;
            let mut vpn = start_vpn;
            while vpn < end_vpn {
                let frame = frame_allocator::allocate()?;
                self.page_table
                    .map(vpn, frame.ppn(), MapPermission::READ | MapPermission::WRITE | MapPermission::USER, MemoryType::Normal)?;
                let _ = frame.into_ppn();
                vpn = vpn.checked_add(1)?;
            }
        } else if target < self.heap_brk {
            // 向下收缩：逐页 unmap 并释放。
            let start_vpn = target.ceil()?;
            let end_vpn = self.heap_brk.ceil()?;
            let mut vpn = start_vpn;
            while vpn < end_vpn {
                if let Ok(ppn) = self.page_table.unmap(vpn) {
                    // SAFETY: 堆页是独占 Framed 映射，不参与 COW 共享。
                    unsafe { frame_allocator::deallocate(ppn) };
                }
                vpn = vpn.checked_add(1).expect("brk shrink vpn overflow");
            }
        }
        self.heap_brk = target;
        Ok(self.heap_brk.value())
    }

    /// 把已映射的用户可读页复制到内核拥有的缓冲区，不向外暴露用户页借用。
    /// COW 页虽然暂时不可写，但仍保留 READ|USER 权限，可正常读取。
    pub(crate) fn copy_from_user(&self, src: usize, len: usize) -> Result<Vec<u8>, MemoryError> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let end = src.checked_add(len).ok_or(MemoryError::AreaNotFound)?;
        let mut page = VirtAddr::new(src).floor();
        let end_page = VirtAddr::new(end - 1).floor();
        loop {
            let (_, perms, _) = self
                .page_table
                .leaf_info(page)
                .ok_or(MemoryError::AreaNotFound)?;
            if !perms.contains(MapPermission::USER | MapPermission::READ) {
                return Err(MemoryError::AreaNotFound);
            }
            if page == end_page {
                break;
            }
            page = page.checked_add(1)?;
        }
        let mut data = alloc::vec![0u8; len];
        let mut copied = 0;
        while copied < len {
            let va = src + copied;
            let vpn = VirtAddr::new(va).floor();
            let ppn = self
                .page_table
                .translate(vpn)
                .ok_or(MemoryError::AreaNotFound)?;
            let offset = va & (super::address::PAGE_SIZE - 1);
            let n = (super::address::PAGE_SIZE - offset).min(len - copied);
            let phys = (ppn.value() << super::address::PAGE_SHIFT) + offset;
            // SAFETY: 整个范围已确认是用户可读叶子页；身份映射使物理页在内核可读。
            unsafe {
                core::ptr::copy_nonoverlapping(phys as *const u8, data[copied..].as_mut_ptr(), n)
            };
            copied += n;
        }
        Ok(data)
    }

    /// 从内核缓冲区写入当前地址空间的用户页，包含跨页与 COW 处理。
    /// 先检查全部目的页，避免地址错误导致部分写入；写入走物理页身份映射，
    /// 不依赖 S-mode 的 SUM 位，也不会在内核写入时触发用户页的 store fault。
    pub(crate) fn copy_to_user(&mut self, dst: usize, data: &[u8]) -> Result<(), MemoryError> {
        if data.is_empty() {
            return Ok(());
        }
        self.prepare_user_write(dst, data.len())?;
        let mut copied = 0;
        while copied < data.len() {
            let va = dst + copied;
            let vpn = VirtAddr::new(va).floor();
            let ppn = self
                .page_table
                .translate(vpn)
                .ok_or(MemoryError::AreaNotFound)?;
            let offset = va & (super::address::PAGE_SIZE - 1);
            let n = (super::address::PAGE_SIZE - offset).min(data.len() - copied);
            let phys = (ppn.value() << super::address::PAGE_SHIFT) + offset;
            // SAFETY: 叶子已校验为用户可写且 COW 已解除；物理页由当前地址空间持有。
            unsafe { core::ptr::copy_nonoverlapping(data[copied..].as_ptr(), phys as *mut u8, n) };
            copied += n;
        }
        Ok(())
    }

    /// 在消费外部输入前校验并预先解除目的范围的 COW；不写入任何用户字节。
    /// 同一任务在 syscall 内不会与其他 hart 并发改写地址空间（M2 单核约束）。
    pub(crate) fn prepare_user_write(&mut self, dst: usize, len: usize) -> Result<(), MemoryError> {
        if len == 0 {
            return Ok(());
        }
        let end = dst.checked_add(len).ok_or(MemoryError::AreaNotFound)?;
        let mut page = VirtAddr::new(dst).floor();
        let end_page = VirtAddr::new(end - 1).floor();
        loop {
            let (_, perms, _) = self
                .page_table
                .leaf_info(page)
                .ok_or(MemoryError::AreaNotFound)?;
            if !perms.contains(MapPermission::USER)
                || !(perms.contains(MapPermission::WRITE) || self.cow_pages.contains(&page))
            {
                return Err(MemoryError::AreaNotFound);
            }
            if page == end_page {
                break;
            }
            page = page.checked_add(1)?;
        }
        // 分配失败也不能产生半写：先把所有 COW 页解除，再复制任何字节。
        let mut page = VirtAddr::new(dst).floor();
        loop {
            self.handle_cow_store(page)?;
            if page == end_page {
                break;
            }
            page = page.checked_add(1)?;
        }
        Ok(())
    }

    /// 内核映像身份映射（代码/rodata/data/bss/boot 栈/空闲 RAM/必需 MMIO）。
    fn insert_kernel_mapping(
        &mut self,
        layout: KernelImageLayout,
        physical_memory_end: VirtAddr,
    ) -> Result<(), MemoryError> {
        self.insert(MapArea::new(
            layout.text,
            MapType::Identity,
            MapPermission::READ | MapPermission::EXECUTE,
        )?)?;
        self.insert(MapArea::new(
            layout.rodata,
            MapType::Identity,
            MapPermission::READ,
        )?)?;
        self.insert(MapArea::new(
            layout.data,
            MapType::Identity,
            MapPermission::READ | MapPermission::WRITE,
        )?)?;
        self.insert(MapArea::new(
            layout.bss,
            MapType::Identity,
            MapPermission::READ | MapPermission::WRITE,
        )?)?;
        self.insert(MapArea::new(
            layout.boot_stack,
            MapType::Identity,
            MapPermission::READ | MapPermission::WRITE,
        )?)?;

        let free_memory = AddressRange::new(layout.end, physical_memory_end)?;
        self.insert(MapArea::new(
            free_memory,
            MapType::Identity,
            MapPermission::READ | MapPermission::WRITE,
        )?)?;
        for &(start, end) in crate::arch::mm::kernel_device_ranges() {
            self.insert(MapArea::new_device(AddressRange::new(
                VirtAddr::new(start),
                VirtAddr::new(end),
            )?)?)?;
        }
        Ok(())
    }

    /// 插入区域，拒绝重叠并在成功建图后才发布到区域表。
    pub fn insert(&mut self, area: MapArea) -> Result<(), MemoryError> {
        if area.range().is_empty() {
            return Ok(());
        }
        if self
            .areas
            .iter()
            .flatten()
            .any(|existing| existing.range().overlaps(area.range()))
        {
            return Err(MemoryError::Overlap);
        }
        let slot = self
            .areas
            .iter()
            .position(Option::is_none)
            .ok_or(MemoryError::AreaLimit)?;
        area.map_into(&mut self.page_table)?;
        self.areas[slot] = Some(area);
        Ok(())
    }

    /// 按区域起始虚拟地址删除映射并释放其拥有的数据页。
    pub fn remove(&mut self, start: VirtAddr) -> Result<(), MemoryError> {
        let start = start.floor();
        let slot = self
            .areas
            .iter()
            .position(|area| area.is_some_and(|area| area.range().start() == start))
            .ok_or(MemoryError::AreaNotFound)?;
        let area = self.areas[slot].take().expect("area slot changed");
        area.unmap_from(&mut self.page_table)
    }

    /// 查询虚拟地址当前映射到的物理页；返回值不包含页内偏移。
    pub fn translate(&self, virtual_address: VirtAddr) -> Option<PhysPageNum> {
        self.page_table.translate(virtual_address.floor())
    }

    /// 返回交给架构 MMU 激活接口的根物理页号。
    pub const fn root_ppn(&self) -> PhysPageNum {
        self.page_table.root_ppn()
    }

    /// 返回已发布的映射区域数量，仅用于状态检查和日志。
    pub fn area_count(&self) -> usize {
        self.areas.iter().flatten().count()
    }
}

/// 用户 LOAD 段按页合并为不相交的映射区间（排序 + 合并重叠/相邻页，权限并集）。
fn merged_user_ranges(
    segments: &[crate::kernel::elf::LoadSegment],
) -> Vec<(VirtPageNum, VirtPageNum, MapPermission)> {
    let mut items: Vec<(usize, usize, MapPermission)> = segments
        .iter()
        .filter(|seg| seg.mem_bytes > 0)
        .map(|seg| {
            let start = VirtAddr::new(seg.vaddr).floor();
            let end = VirtAddr::new(seg.vaddr + seg.mem_bytes)
                .ceil()
                .expect("user segment ceil overflow");
            (start.value(), end.value(), segment_permissions(seg.flags))
        })
        .collect();
    items.sort_by_key(|item| item.0);
    let mut merged: Vec<(usize, usize, MapPermission)> = Vec::new();
    for (start, end, perms) in items {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = core::cmp::max(last.1, end);
                last.2 = last.2.union(perms);
                continue;
            }
        }
        merged.push((start, end, perms));
    }
    merged
        .into_iter()
        .map(|(start, end, perms)| (VirtPageNum::new(start), VirtPageNum::new(end), perms))
        .collect()
}

/// 从 ELF 段标志推导公共映射权限（用户访问恒开，写权限隐含读，见 page_table 校验）。
fn segment_permissions(flags: crate::kernel::elf::SegmentFlags) -> MapPermission {
    let mut p = MapPermission::READ | MapPermission::USER;
    if flags.write {
        p = p.union(MapPermission::WRITE);
    }
    if flags.execute {
        p = p.union(MapPermission::EXECUTE);
    }
    p
}

impl Drop for MemorySet {
    fn drop(&mut self) {
        let mut cleanup_failed = false;
        for slot in &mut self.areas {
            if let Some(area) = slot.take() {
                cleanup_failed |= area.unmap_from(&mut self.page_table).is_err();
            }
        }
        assert!(!cleanup_failed, "MemorySet cleanup found a missing mapping");
    }
}

/// 启动自检：仅操作未激活的临时页表，不影响当前内核地址空间。
/// 验证跨架构 PTE 权限编解码、Framed 数据页 owner 与撤销后的资源回收。
pub(crate) fn self_test() {
    let baseline = frame_allocator::allocated_count();
    let start = VirtAddr::new(0x4000_0000);
    let next = VirtAddr::new(0x4000_1000);
    let end = VirtAddr::new(0x4000_2000);
    let area = MapArea::new(
        AddressRange::new(start, end).expect("MemorySet self-test range"),
        MapType::Framed,
        MapPermission::READ | MapPermission::WRITE | MapPermission::USER,
    )
    .expect("MemorySet self-test area");
    {
        let mut space = MemorySet::new_bare().expect("MemorySet self-test root");
        space.insert(area).expect("MemorySet self-test map");
        assert_eq!(space.area_count(), 1);
        let first = space
            .translate(start)
            .expect("MemorySet self-test first PTE");
        let second = space
            .translate(next)
            .expect("MemorySet self-test second PTE");
        assert_ne!(first, second, "MemorySet mapped two VPNs to one frame");
        for address in [start, next] {
            let (_, permissions, memory_type) = space
                .page_table
                .leaf_info(address.floor())
                .expect("MemorySet self-test leaf info");
            assert_eq!(
                permissions,
                MapPermission::READ | MapPermission::WRITE | MapPermission::USER
            );
            assert_eq!(memory_type, MemoryType::Normal);
        }
        assert_eq!(space.insert(area), Err(MemoryError::Overlap));
        space.remove(start).expect("MemorySet self-test unmap");
        assert_eq!(space.area_count(), 0);
        assert!(space.translate(start).is_none());
        assert!(space.translate(next).is_none());
        assert_eq!(space.remove(start), Err(MemoryError::AreaNotFound));
    }
    assert_eq!(
        frame_allocator::allocated_count(),
        baseline,
        "MemorySet self-test leaked frames"
    );
    crate::log_info!("M3-MEMORYSET-PASS");
}

/// M2.8 定向自检：逐个物理帧分配点注入一次 OOM，验证 COW fork 原子性与释放。
/// 独立测试地址空间不进入调度器，运行时只占用 bootstrap hart。
#[cfg(all(feature = "m28-frame-fault-test", feature = "arch-riscv64"))]
pub(crate) fn test_cow_allocation_failures() {
    fn fixture(layout: KernelImageLayout, memory_end: VirtAddr) -> MemorySet {
        let mut space = MemorySet::new_kernel(layout, memory_end).expect("OOM fixture kernel map");
        let areas = [
            (
                0x10000,
                0x11000,
                MapPermission::READ | MapPermission::EXECUTE | MapPermission::USER,
            ),
            (
                0x11000,
                0x13000,
                MapPermission::READ | MapPermission::WRITE | MapPermission::USER,
            ),
            (
                0x7efff000,
                0x7f000000,
                MapPermission::READ | MapPermission::WRITE | MapPermission::USER,
            ),
        ];
        for (start, end, permissions) in areas {
            let range = AddressRange::new(VirtAddr::new(start), VirtAddr::new(end)).unwrap();
            space
                .insert(MapArea::new(range, MapType::Framed, permissions).unwrap())
                .expect("OOM fixture user map");
        }
        space
    }

    let layout = KernelImageLayout::current();
    let (_, physical_end) =
        crate::arch::mm::physical_memory_range(crate::kernel::mm::configured_memory_bytes())
            .expect("OOM fixture physical range");
    let memory_end = VirtAddr::new(physical_end);
    let baseline_frames = frame_allocator::allocated_count();
    let baseline_refs = frame_allocator::test_shared_ref_entries();

    // 正常复制的实际分配次数决定完整故障点范围，不依赖页表布局常数。
    let mut parent = fixture(layout, memory_end);
    frame_allocator::set_test_failure_budget(None);
    let child = parent
        .dup_cow(layout, memory_end)
        .expect("OOM fixture control fork");
    let allocation_points = frame_allocator::test_allocation_attempts();
    assert!(allocation_points > 0);
    drop(child);
    drop(parent);
    assert_eq!(frame_allocator::allocated_count(), baseline_frames);
    assert_eq!(frame_allocator::test_shared_ref_entries(), baseline_refs);

    let sample_vpns =
        [0x10000, 0x11000, 0x12000, 0x7efff000].map(|address| VirtAddr::new(address).floor());
    for budget in 0..allocation_points {
        let mut parent = fixture(layout, memory_end);
        let before = sample_vpns.map(|vpn| parent.page_table.leaf_info(vpn).unwrap());
        let frames_with_parent = frame_allocator::allocated_count();
        frame_allocator::set_test_failure_budget(Some(budget));
        let failed = parent.dup_cow(layout, memory_end).is_err();
        assert!(
            failed,
            "COW fork succeeded at injected allocation point {}",
            budget
        );
        assert_eq!(frame_allocator::test_allocation_attempts(), budget + 1);
        assert_eq!(
            frame_allocator::allocated_count(),
            frames_with_parent,
            "COW fork leaked frames at allocation point {}",
            budget
        );
        assert!(
            parent.cow_pages.is_empty(),
            "failed fork changed parent COW set"
        );
        for (vpn, expected) in sample_vpns.into_iter().zip(before) {
            assert_eq!(
                parent.page_table.leaf_info(vpn).unwrap(),
                expected,
                "failed fork changed parent leaf at allocation point {}",
                budget
            );
        }
        drop(parent);
        assert_eq!(frame_allocator::allocated_count(), baseline_frames);
        assert_eq!(frame_allocator::test_shared_ref_entries(), baseline_refs);
    }
    crate::log_info!("M28-FRAME-OOM-PASS points={}", allocation_points);

    // 第二个跨页 COW 分配失败时，第一页面映射可以提前私有化，用户字节不能半写。
    let mut parent = fixture(layout, memory_end);
    let child = parent
        .dup_cow(layout, memory_end)
        .expect("COW write fixture fork");
    let dst = 0x11fff;
    frame_allocator::set_test_failure_budget(Some(1));
    assert!(parent.copy_to_user(dst, &[0x5a, 0xa5]).is_err());
    assert_eq!(frame_allocator::test_allocation_attempts(), 2);
    assert_eq!(parent.copy_from_user(dst, 2).unwrap(), [0, 0]);
    assert_eq!(child.copy_from_user(dst, 2).unwrap(), [0, 0]);
    parent
        .copy_to_user(dst, &[0x5a, 0xa5])
        .expect("COW write retry");
    assert_eq!(parent.copy_from_user(dst, 2).unwrap(), [0x5a, 0xa5]);
    assert_eq!(child.copy_from_user(dst, 2).unwrap(), [0, 0]);
    drop(child);
    drop(parent);
    assert_eq!(frame_allocator::allocated_count(), baseline_frames);
    assert_eq!(frame_allocator::test_shared_ref_entries(), baseline_refs);
    crate::log_info!("M28-COW-WRITE-OOM-PASS");
}

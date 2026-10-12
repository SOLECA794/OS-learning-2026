//! 映射区与地址空间所有权。

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
    /// 每个虚拟页由 MemorySet 分配并独占一个物理页。
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
                    // SAFETY: 该叶子映射刚被移除，数据页所有权回到本失败路径。
                    unsafe { frame_allocator::deallocate(ppn) };
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
                    // SAFETY: 页表叶子已清除，且 framed 区域是该页唯一 owner。
                    unsafe { frame_allocator::deallocate(ppn) };
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
}

impl MemorySet {
    /// 创建只有空根页表、尚未包含任何映射的地址空间。
    pub fn new_bare() -> Result<Self, MemoryError> {
        Ok(Self {
            page_table: PageTable::new()?,
            areas: [None; MAX_MAP_AREAS],
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

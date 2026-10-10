//! 映射区与地址空间所有权。

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
        memory_set.insert(MapArea::new(
            layout.text,
            MapType::Identity,
            MapPermission::READ | MapPermission::EXECUTE,
        )?)?;
        memory_set.insert(MapArea::new(
            layout.rodata,
            MapType::Identity,
            MapPermission::READ,
        )?)?;
        memory_set.insert(MapArea::new(
            layout.data,
            MapType::Identity,
            MapPermission::READ | MapPermission::WRITE,
        )?)?;
        memory_set.insert(MapArea::new(
            layout.bss,
            MapType::Identity,
            MapPermission::READ | MapPermission::WRITE,
        )?)?;
        memory_set.insert(MapArea::new(
            layout.boot_stack,
            MapType::Identity,
            MapPermission::READ | MapPermission::WRITE,
        )?)?;

        let free_memory = AddressRange::new(layout.end, physical_memory_end)?;
        memory_set.insert(MapArea::new(
            free_memory,
            MapType::Identity,
            MapPermission::READ | MapPermission::WRITE,
        )?)?;
        for &(start, end) in crate::arch::mm::kernel_device_ranges() {
            memory_set.insert(MapArea::new_device(AddressRange::new(
                VirtAddr::new(start),
                VirtAddr::new(end),
            )?)?)?;
        }
        Ok(memory_set)
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

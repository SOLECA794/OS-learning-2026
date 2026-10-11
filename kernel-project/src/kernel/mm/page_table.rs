//! 架构无关的三级页表遍历与页表页所有权。

use crate::arch::{self, MemoryType, PagePermissions};

use super::address::{PhysPageNum, VirtPageNum, PAGE_SHIFT, PAGE_SIZE};
use super::frame_allocator::{self, FrameAllocError};

/// 每一级页表包含的 PTE 数量。
const ENTRIES_PER_TABLE: usize = 1 << arch::mm::PAGE_INDEX_BITS;

/// 页表操作可能返回的错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTableError {
    /// 权限为空，或出现硬件不接受的“可写但不可读”组合。
    InvalidPermissions,
    /// 目标虚拟页已经存在有效叶子映射。
    AlreadyMapped,
    /// 目标页或遍历路径不存在。
    NotMapped,
    /// 中间层意外出现叶子项，页表树结构已经不符合预期。
    CorruptPageTable,
    /// 创建页表层级时无法取得物理页。
    Frame(FrameAllocError),
}

impl From<FrameAllocError> for PageTableError {
    fn from(value: FrameAllocError) -> Self {
        Self::Frame(value)
    }
}

/// 架构无关的叶子映射权限位集合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapPermission(u8);

impl MapPermission {
    /// 允许读取。
    pub const READ: Self = Self(1 << 0);
    /// 允许写入；当前公共契约要求写权限同时包含读权限。
    pub const WRITE: Self = Self(1 << 1);
    /// 允许取指执行。
    pub const EXECUTE: Self = Self(1 << 2);
    /// 允许用户态访问。
    pub const USER: Self = Self(1 << 3);

    /// 判断当前权限是否完整包含另一组权限。
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// 合并两组权限，不执行合法性校验。
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// 在写入 PTE 前校验公共权限组合。
    fn validate(self) -> Result<(), PageTableError> {
        if self.0 == 0 || self.contains(Self::WRITE) && !self.contains(Self::READ) {
            Err(PageTableError::InvalidPermissions)
        } else {
            Ok(())
        }
    }

    /// 把公共权限和内存类型收敛到架构 PTE 编码器的窄接口。
    fn arch_permissions(self, memory_type: MemoryType) -> PagePermissions {
        PagePermissions {
            read: self.contains(Self::READ),
            write: self.contains(Self::WRITE),
            execute: self.contains(Self::EXECUTE),
            user: self.contains(Self::USER),
            memory_type,
        }
    }
}

impl core::ops::BitOr for MapPermission {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

/// 独占一棵多级页表树的根页。
pub struct PageTable {
    root: PhysPageNum,
}

impl PageTable {
    /// 分配并清零根页表。
    pub fn new() -> Result<Self, PageTableError> {
        let root = frame_allocator::allocate()?.into_ppn();
        Ok(Self { root })
    }

    /// 返回根页表所在物理页号。
    pub const fn root_ppn(&self) -> PhysPageNum {
        self.root
    }

    /// 为单个虚拟页创建叶子映射，按需分配缺失的中间页表。
    pub fn map(
        &mut self,
        vpn: VirtPageNum,
        ppn: PhysPageNum,
        permissions: MapPermission,
        memory_type: MemoryType,
    ) -> Result<(), PageTableError> {
        permissions.validate()?;
        let entry = self.walk_create(vpn)?;
        if entry.is_valid() {
            return Err(PageTableError::AlreadyMapped);
        }
        *entry =
            arch::mm::PageTableEntry::leaf(ppn.value(), permissions.arch_permissions(memory_type));
        Ok(())
    }

    /// 只读遍历页表并查询叶子物理页号。
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PhysPageNum> {
        let indexes = vpn_indexes(vpn);
        let mut table = self.root;
        for (level, index) in indexes.into_iter().enumerate() {
            let entry = table_ref(table)[index];
            if !entry.is_valid() {
                return None;
            }
            if level + 1 == arch::mm::PAGE_LEVELS {
                return (!entry.is_table()).then(|| PhysPageNum::new(entry.ppn()));
            }
            if !entry.is_table() {
                return None;
            }
            table = PhysPageNum::new(entry.ppn());
        }
        None
    }

    /// 清除单个叶子映射并把原物理页号交还给上层 owner。
    pub fn unmap(&mut self, vpn: VirtPageNum) -> Result<PhysPageNum, PageTableError> {
        let entry = self.walk_existing_mut(vpn)?;
        if !entry.is_valid() || entry.is_table() {
            return Err(PageTableError::NotMapped);
        }
        let ppn = PhysPageNum::new(entry.ppn());
        *entry = arch::mm::PageTableEntry::empty();
        Ok(ppn)
    }

    /// 遍历到叶子槽位，并为缺失的中间层分配页表页。
    fn walk_create(
        &mut self,
        vpn: VirtPageNum,
    ) -> Result<&mut arch::mm::PageTableEntry, PageTableError> {
        let indexes = vpn_indexes(vpn);
        let mut table = self.root;
        for index in indexes.iter().take(arch::mm::PAGE_LEVELS - 1) {
            let current = table_ref(table)[*index];
            if !current.is_valid() {
                let child = frame_allocator::allocate()?.into_ppn();
                table_mut(table)[*index] = arch::mm::PageTableEntry::table(child.value());
                table = child;
            } else if current.is_table() {
                table = PhysPageNum::new(current.ppn());
            } else {
                return Err(PageTableError::CorruptPageTable);
            }
        }
        Ok(&mut table_mut(table)[indexes[arch::mm::PAGE_LEVELS - 1]])
    }

    /// 沿已有中间页表遍历到叶子槽位，不创建任何页面。
    fn walk_existing_mut(
        &mut self,
        vpn: VirtPageNum,
    ) -> Result<&mut arch::mm::PageTableEntry, PageTableError> {
        let indexes = vpn_indexes(vpn);
        let mut table = self.root;
        for index in indexes.iter().take(arch::mm::PAGE_LEVELS - 1) {
            let entry = table_ref(table)[*index];
            if !entry.is_table() {
                return Err(PageTableError::NotMapped);
            }
            table = PhysPageNum::new(entry.ppn());
        }
        Ok(&mut table_mut(table)[indexes[arch::mm::PAGE_LEVELS - 1]])
    }
}

impl Drop for PageTable {
    fn drop(&mut self) {
        unsafe fn release_table(ppn: PhysPageNum, level: usize) {
            if level + 1 < arch::mm::PAGE_LEVELS {
                for entry in table_ref(ppn) {
                    if entry.is_table() {
                        // SAFETY: 每个中间页表只有一个父目录项，递归只访问一次。
                        unsafe { release_table(PhysPageNum::new(entry.ppn()), level + 1) };
                    }
                }
            }
            // SAFETY: 子页表已释放，当前页不再能从页表树到达。
            unsafe { frame_allocator::deallocate(ppn) };
        }

        // SAFETY: PageTable 独占整棵页表树；叶子数据页由 MapArea owner 另行释放。
        unsafe { release_table(self.root, 0) };
    }
}

/// 把虚拟页号拆成从根层到叶子层的页表索引。
fn vpn_indexes(vpn: VirtPageNum) -> [usize; arch::mm::PAGE_LEVELS] {
    let mut indexes = [0; arch::mm::PAGE_LEVELS];
    let mut value = vpn.value();
    let mask = ENTRIES_PER_TABLE - 1;
    let mut level = arch::mm::PAGE_LEVELS;
    while level > 0 {
        level -= 1;
        indexes[level] = value & mask;
        value >>= arch::mm::PAGE_INDEX_BITS;
    }
    indexes
}

/// 通过内核物理内存映射只读访问一个页表页。
fn table_ref(ppn: PhysPageNum) -> &'static [arch::mm::PageTableEntry; ENTRIES_PER_TABLE] {
    let physical = ppn.value() << PAGE_SHIFT;
    let virtual_address = arch::mm::phys_to_virt(physical);
    // SAFETY: 页表页由 PageTable 独占并保持页对齐；PTE 为 8 字节，共占一页。
    unsafe { &*(virtual_address as *const [arch::mm::PageTableEntry; ENTRIES_PER_TABLE]) }
}

/// 在 `&mut PageTable` 独占条件下可变访问一个页表页。
fn table_mut(ppn: PhysPageNum) -> &'static mut [arch::mm::PageTableEntry; ENTRIES_PER_TABLE] {
    assert_eq!(
        core::mem::size_of::<arch::mm::PageTableEntry>() * ENTRIES_PER_TABLE,
        PAGE_SIZE
    );
    let physical = ppn.value() << PAGE_SHIFT;
    let virtual_address = arch::mm::phys_to_virt(physical);
    // SAFETY: 调用路径持有 &mut PageTable，同一页表页不会同时产生另一可变引用。
    unsafe { &mut *(virtual_address as *mut [arch::mm::PageTableEntry; ENTRIES_PER_TABLE]) }
}

//! RISC-V Sv39 页表编码。

use super::super::PagePermissions;

pub const PAGE_LEVELS: usize = 3;
pub const PAGE_INDEX_BITS: usize = 9;
const PHYS_MEMORY_BASE: usize = 0x8000_0000;

const PTE_VALID: usize = 1 << 0;
const PTE_READ: usize = 1 << 1;
const PTE_WRITE: usize = 1 << 2;
const PTE_EXECUTE: usize = 1 << 3;
const PTE_USER: usize = 1 << 4;
const PTE_ACCESSED: usize = 1 << 6;
const PTE_DIRTY: usize = 1 << 7;
const PTE_LEAF_MASK: usize = PTE_READ | PTE_WRITE | PTE_EXECUTE;

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PageTableEntry(usize);

impl PageTableEntry {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn table(ppn: usize) -> Self {
        Self((ppn << 10) | PTE_VALID)
    }

    pub fn leaf(ppn: usize, permissions: PagePermissions) -> Self {
        let mut bits = (ppn << 10) | PTE_VALID | PTE_ACCESSED;
        if permissions.read {
            bits |= PTE_READ;
        }
        if permissions.write {
            bits |= PTE_WRITE | PTE_DIRTY;
        }
        if permissions.execute {
            bits |= PTE_EXECUTE;
        }
        if permissions.user {
            bits |= PTE_USER;
        }
        Self(bits)
    }

    pub const fn is_valid(self) -> bool {
        self.0 & PTE_VALID != 0
    }

    pub const fn is_table(self) -> bool {
        self.is_valid() && self.0 & PTE_LEAF_MASK == 0
    }

    pub const fn ppn(self) -> usize {
        self.0 >> 10
    }

    /// 把叶子 PTE 读回为公共权限与内存类型（memory_type 未编码于 Sv39 PTE，
    /// 恒为 Normal；内核设备区由 Identity 区域承载，不经过本路径）。
    pub fn permissions(self) -> PagePermissions {
        PagePermissions {
            read: self.0 & PTE_READ != 0,
            write: self.0 & PTE_WRITE != 0,
            execute: self.0 & PTE_EXECUTE != 0,
            user: self.0 & PTE_USER != 0,
            memory_type: super::super::MemoryType::Normal,
        }
    }
}

/// 当前 QEMU virt 的连续 RAM 范围。
pub fn physical_memory_range(memory_bytes: usize) -> Option<(usize, usize)> {
    PHYS_MEMORY_BASE
        .checked_add(memory_bytes)
        .map(|end| (PHYS_MEMORY_BASE, end))
}

/// 刷新本 hart 全部翻译缓存（PTE 修改后调用；不切换根页表）。
pub fn flush_tlb() {
    // SAFETY: sfence.vma zero, zero 只丢弃本核全部 TLB 项，无其它副作用。
    unsafe {
        core::arch::asm!("sfence.vma zero, zero", options(nostack, preserves_flags));
    }
}

/// 内核 Sv39 页表保持 RAM 身份映射，因此可直接访问页表页。
/// 移除条件：切换到高半内核直接映射后加入固定偏移。
pub const fn phys_to_virt(physical_address: usize) -> usize {
    physical_address
}

pub const fn kernel_device_ranges() -> &'static [(usize, usize)] {
    super::platform::qemu_virt::DEVICE_RANGES
}

/// 激活 Sv39 根页表并刷新本 hart 的全部 TLB。
///
/// # Safety
/// root_ppn 必须指向有效且覆盖当前代码、栈和所有后续访存的 Sv39 根页表。
pub unsafe fn activate(root_ppn: usize) {
    const SATP_MODE_SV39: usize = 8usize << 60;
    let satp = SATP_MODE_SV39 | root_ppn;
    // SAFETY: 契约由调用方保证；切换前后的当前执行地址均为身份映射。
    unsafe {
        core::arch::asm!(
            "fence rw, rw",
            "csrw satp, {satp}",
            "sfence.vma zero, zero",
            satp = in(reg) satp,
            options(nostack),
        );
    }
}

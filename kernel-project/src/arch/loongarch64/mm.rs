//! LoongArch64 三级页表编码。
//!
//! 位定义来自《LoongArch Reference Manual, Volume 1》的普通页表项格式：
//! V[0]、D[1]、PLV[3:2]、MAT[5:4]、G[6]、P[7]、W[8]、NR[61]、NX[62]。

use core::arch::{asm, global_asm};

use super::super::{MemoryType, PagePermissions};

pub const PAGE_LEVELS: usize = 3;
pub const PAGE_INDEX_BITS: usize = 9;
const LOW_MEMORY_LIMIT: usize = 0x1000_0000;

const PTE_VALID: usize = 1 << 0;
const PTE_DIRTY: usize = 1 << 1;
const PTE_PLV3: usize = 3 << 2;
const PTE_MAT_COHERENT: usize = 1 << 4;
const PTE_HUGE: usize = 1 << 6;
const PTE_PRESENT: usize = 1 << 7;
const PTE_WRITE: usize = 1 << 8;
const PTE_NO_READ: usize = 1 << 61;
const PTE_NO_EXECUTE: usize = 1 << 62;
const PHYSICAL_ADDRESS_MASK: usize = ((1usize << 61) - 1) & !0xfff;

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PageTableEntry(usize);

impl PageTableEntry {
    pub const fn empty() -> Self {
        Self(0)
    }

    /// 普通目录项保存下一级页表物理地址；bit 6 为 0 表示它不是大页。
    pub const fn table(ppn: usize) -> Self {
        Self(ppn << 12)
    }

    pub fn leaf(ppn: usize, permissions: PagePermissions) -> Self {
        let mut bits = (ppn << 12) | PTE_VALID | PTE_PRESENT;
        if permissions.memory_type == MemoryType::Normal {
            bits |= PTE_MAT_COHERENT;
        }
        if permissions.write {
            bits |= PTE_WRITE | PTE_DIRTY;
        }
        if permissions.user {
            bits |= PTE_PLV3;
        }
        if !permissions.read {
            bits |= PTE_NO_READ;
        }
        if !permissions.execute {
            bits |= PTE_NO_EXECUTE;
        }
        Self(bits)
    }

    pub const fn is_valid(self) -> bool {
        self.0 != 0
    }

    pub const fn is_table(self) -> bool {
        self.is_valid() && self.0 & PTE_HUGE == 0 && self.0 & PTE_PRESENT == 0
    }

    pub const fn ppn(self) -> usize {
        (self.0 & PHYSICAL_ADDRESS_MASK) >> 12
    }
}

/// QEMU LoongArch virt 当前只把低 256 MiB 建模为连续低端 RAM。
pub fn physical_memory_range(memory_bytes: usize) -> Option<(usize, usize)> {
    if memory_bytes == 0 || memory_bytes > LOW_MEMORY_LIMIT {
        return None;
    }
    Some((0, memory_bytes))
}

/// 内核页表保持 RAM 身份映射，因此分页切换前后都可用物理地址访问页表页。
/// 移除条件：切换到高半内核直接映射后加入固定偏移。
pub const fn phys_to_virt(physical_address: usize) -> usize {
    physical_address
}

/// QEMU virt 平台在分页后仍需访问的设备页。
pub const fn kernel_device_ranges() -> &'static [(usize, usize)] {
    &[
        (0x100e_0000, 0x100e_1000), // ACPI GED reset/poweroff
        (0x1fe0_0000, 0x1fe0_1000), // NS16550A UART
    ]
}

global_asm!(
    r#"
    .section .text.tlb_refill, "ax"
    .align 12
    .globl __loongarch64_tlb_refill
__loongarch64_tlb_refill:
    csrwr $r12, 0x8b
    csrrd $r12, 0x1b
    lddir $r12, $r12, 3
    lddir $r12, $r12, 1
    ldpte $r12, 0
    ldpte $r12, 1
    tlbfill
    csrrd $r12, 0x8b
    ertn
"#
);

/// 配置三级页表、TLB refill 入口并进入页映射模式。
///
/// # Safety
/// root_ppn 必须指向有效且覆盖当前代码、栈、BSS 与设备访问的根页表。
pub unsafe fn activate(root_ppn: usize) {
    unsafe extern "C" {
        fn __loongarch64_tlb_refill();
    }

    const PWCL: usize = 12 | (9 << 5) | (21 << 10) | (9 << 15) | (30 << 20) | (9 << 25);
    const CRMD_VALUE: usize = (1 << 4) | (1 << 5) | (1 << 7);
    const CRMD_MASK: usize = (1 << 3) | (1 << 4) | (3 << 5) | (3 << 7);
    let root = root_ppn << 12;
    let refill = __loongarch64_tlb_refill as *const () as usize;

    // SAFETY: 所有地址在切换前为物理直址、切换后为页表身份映射；refill 入口是物理地址。
    unsafe {
        asm!(
            "csrwr {root}, 0x19",
            "csrwr $r0, 0x1a",
            "csrwr {pwcl}, 0x1c",
            "csrwr $r0, 0x1d",
            "csrwr {page_shift}, 0x1e",
            "csrwr {page_shift}, 0x8e",
            "csrwr {refill}, 0x88",
            "invtlb 0x0, $r0, $r0",
            "dbar 0",
            "csrxchg {crmd_value}, {crmd_mask}, 0x0",
            "ibar 0",
            root = in(reg) root,
            pwcl = in(reg) PWCL,
            page_shift = in(reg) 12usize,
            refill = in(reg) refill,
            crmd_value = in(reg) CRMD_VALUE,
            crmd_mask = in(reg) CRMD_MASK,
            options(nostack),
        );
    }
}

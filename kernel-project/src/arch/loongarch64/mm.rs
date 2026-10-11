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

/// QEMU 9.2.1 LoongArch virt 要求 ram_size >= 1 GiB 才能启动（M0 实测），
/// 但地址空间是分段布局：RAM = 低 256 MiB [0, 0x1000_0000) + 高内存 [0x9000_0000, ...)，
/// 中间是设备 MMIO 区（UART 0x1fe0_0000 / ACPI 0x100e_0000，见 kernel_device_ranges）。
/// 内核只消费低 256 MiB 连续段；高内存与设备区留待后续接入（不可把 [0, ram_size)
/// 全当 RAM，否则 free frames 会覆盖设备 MMIO）。
pub fn physical_memory_range(memory_bytes: usize) -> Option<(usize, usize)> {
    if memory_bytes < LOW_MEMORY_LIMIT {
        return None;
    }
    Some((0, LOW_MEMORY_LIMIT))
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
    //
    // 汇编约束（P0② 修复，规划书 2026-10-12）：CSRWR/CSRXCHG 会把旧 CSR 值写回
    // 源寄存器，因此所有被覆盖的操作数必须声明 inlateout（值被覆盖且不再使用），
    // 不能声明 in(reg)（in 约束要求汇编结束后输入寄存器仍保持原值）。
    // 0x1e 与 0x8e 两个页大小 CSR 各用独立寄存器装载 12：第一次 CSRWR 后该寄存器
    // 已变成旧 CSR 值，第二次写入必须重新装载，否则写的是垃圾值（原实现的隐患）。
    unsafe {
        asm!(
            "csrwr {root}, 0x19",
            "csrwr $r0, 0x1a",
            "csrwr {pwcl}, 0x1c",
            "csrwr $r0, 0x1d",
            "csrwr {ps}, 0x1e",
            "csrwr {ps2}, 0x8e",
            "csrwr {refill}, 0x88",
            "invtlb 0x0, $r0, $r0",
            "dbar 0",
            "csrxchg {crmd_value}, {crmd_mask}, 0x0",
            "ibar 0",
            root = inlateout(reg) root => _,
            pwcl = inlateout(reg) PWCL => _,
            ps = inlateout(reg) 12usize => _,
            ps2 = inlateout(reg) 12usize => _,
            refill = inlateout(reg) refill => _,
            crmd_value = inlateout(reg) CRMD_VALUE => _,
            crmd_mask = in(reg) CRMD_MASK,
            options(nostack),
        );
    }
}

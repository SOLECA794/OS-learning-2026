//! QEMU virt 固定设备布局（当前 build.mk 固定 `-machine virt` 且未启用 AIA）。
//!
//! 来源：QEMU `hw/riscv/virt.c`、`include/hw/riscv/virt.h`；只服务当前
//! QEMU 平台。添加实板时必须从平台描述选择，不能复用这些地址。

pub const UART0: usize = 0x1000_0000;
pub const UART0_IRQ: u32 = 10;
pub const PLIC: usize = 0x0c00_0000;

// QEMU virt 每个 hart 有 M、S 两个 PLIC context：hart n 的 S context = 2n+1。
// bootstrap hart 由启动选主决定，不能固定使用 hart 0 的 context。
pub const HART_COUNT: usize = 8; // 与 boot.rs / linker/riscv64.ld 的 boot 栈数量一致

pub const fn plic_s_enable(hartid: usize) -> usize {
    PLIC + 0x2000 + (2 * hartid + 1) * 0x80
}

pub const fn plic_s_threshold(hartid: usize) -> usize {
    PLIC + 0x200000 + (2 * hartid + 1) * 0x1000
}

pub const fn plic_s_claim(hartid: usize) -> usize {
    plic_s_threshold(hartid) + 4
}

/// 仅映射实际要访问的寄存器页，避免将整个设备窗口暴露进所有用户地址空间。
pub const DEVICE_RANGES: &[(usize, usize)] = &[
    (PLIC, PLIC + 0x1000),
    (PLIC + 0x2000, PLIC + 0x3000),
    // 至多八个 boot hart 的 S context，各占一个独立页。
    (PLIC + 0x201000, PLIC + 0x202000),
    (PLIC + 0x203000, PLIC + 0x204000),
    (PLIC + 0x205000, PLIC + 0x206000),
    (PLIC + 0x207000, PLIC + 0x208000),
    (PLIC + 0x209000, PLIC + 0x20a000),
    (PLIC + 0x20b000, PLIC + 0x20c000),
    (PLIC + 0x20d000, PLIC + 0x20e000),
    (PLIC + 0x20f000, PLIC + 0x210000),
    (UART0, UART0 + 0x1000),
];

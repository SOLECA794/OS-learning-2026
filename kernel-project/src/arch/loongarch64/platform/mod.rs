//! LoongArch64 机器平台选择层。
//!
//! CPU 指令、CSR 与启动约定属于 `arch::loongarch64`；UART、电源控制器等 MMIO
//! 布局属于具体机器平台。两者分开后，QEMU virt 的固定地址不会泄漏到实板后端。

pub(super) mod qemu_virt;

// 实板后端先参与编译以保证代码形状有效，但当前构建目标仍固定为 QEMU virt。
// 移除条件：Kconfig 增加 LoongArch 平台选择后，改为按对应 feature 条件编译。
#[allow(dead_code)]
mod loongson_2k1000;

// 当前 build.mk 把 LoongArch 的 QEMU_MACHINE 固定为 virt，因此此处的唯一有效平台
// 也是 qemu_virt。加入实板构建目标时，平台选择必须与 Kconfig 使用同一真相源。
pub(super) use qemu_virt::{
    power_off, restart, EXTIOI_ENABLE_BASE, EXTIOI_IPMAP_BASE, EXTIOI_ISR_BASE, PCH_PIC_BASE,
    UART_PCH_IRQ,
};

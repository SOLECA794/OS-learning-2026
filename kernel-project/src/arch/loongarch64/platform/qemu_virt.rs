//! QEMU 10.0.2 LoongArch `virt` 机器的平台操作。
//!
//! 比赛容器中的 `qemu-system-loongarch64` 使用 ACPI GED（Generic Event Device）提供
//! 复位与 S5 关机寄存器。这里使用 QEMU 机器定义的物理地址；当前内核运行在
//! CRMD.DA=1、CRMD.PG=0 的物理直址模式，不能使用已开启 DMW 的内核所用高半地址。

/// ACPI GED 寄存器区物理基址。
///
/// QEMU v10.0.2：VIRT_GED_EVT_ADDR(0x100e0000) + 事件选择区(4 字节)
/// + 内存热插拔区(24 字节) = 0x100e001c。
const GED_REG_BASE: usize = 0x100e_001c;

/// GED Sleep Control 寄存器，8 位访问。
const GED_SLEEP_CONTROL: usize = GED_REG_BASE;
/// GED Reset 寄存器，8 位访问。
const GED_RESET: usize = GED_REG_BASE + 2;

/// ACPI S5：SLP_TYP=5（bits 4:2）并置位 SLP_EN（bit 5）。
const GED_POWER_OFF_VALUE: u8 = (5 << 2) | (1 << 5);
/// QEMU ACPI GED 定义的复位确认值。
const GED_RESET_VALUE: u8 = 0x42;

#[inline]
unsafe fn write_ged(address: usize, value: u8) {
    // SAFETY: 调用方只传入 QEMU virt 已定义的 GED 字节寄存器地址；volatile 防止编译器
    // 删除或合并这次具有外部副作用的 MMIO 写入。
    unsafe { core::ptr::write_volatile(address as *mut u8, value) }
}

/// 请求 QEMU 重启虚拟机。
pub fn restart() -> ! {
    crate::log_info!("restarting QEMU LoongArch virt machine");

    // SAFETY: GED_RESET 是 QEMU virt 的 8 位复位寄存器，0x42 是其规定的触发值。
    unsafe { write_ged(GED_RESET, GED_RESET_VALUE) };

    // QEMU 的复位请求是异步处理的；发出请求后不得继续执行原内核路径。
    super::super::park()
}

/// 请求 QEMU 关闭虚拟机。
pub fn power_off() -> ! {
    crate::log_info!("powering off QEMU LoongArch virt machine");

    // SAFETY: GED_SLEEP_CONTROL 是 QEMU virt 的 8 位 ACPI Sleep Control 寄存器；
    // 0x34 表示 SLP_TYP=S5 且 SLP_EN=1。
    unsafe { write_ged(GED_SLEEP_CONTROL, GED_POWER_OFF_VALUE) };

    // 关机请求同样异步；若平台未响应，则安全地停在 idle 循环中。
    super::super::park()
}

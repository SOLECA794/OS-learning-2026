//! Loongson 2K1000 实板平台占位后端。
//!
//! 保留原因：先固定“架构代码不包含板级 MMIO”的目录与接口边界，为后续实板移植
//! 留出明确位置。
//! 行为边界：当前构建不会选择本后端；即使被内部调用，也只报告未实现并安全停机，
//! 不会用未经验证的地址写实板寄存器。
//! 移除条件：取得目标板准确型号、固件启动协议和电源控制器文档后，实现并通过实板验证；
//! 同时在 Kconfig 中增加平台选择，使 `platform/mod.rs` 按 feature 选择后端。

/// 请求重启 Loongson 2K1000。
pub(super) fn restart() -> ! {
    crate::log_error!("Loongson 2K1000 restart is not implemented; system halted");
    super::super::park()
}

/// 请求关闭 Loongson 2K1000 电源。
pub(super) fn power_off() -> ! {
    crate::log_error!("Loongson 2K1000 power-off is not implemented; system halted");
    super::super::park()
}

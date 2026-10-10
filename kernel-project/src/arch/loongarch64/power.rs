//! LoongArch64 对外统一的电源控制接口。
//!
//! 本模块属于架构层接口：上层只表达“重启”或“关机”，不感知当前运行在 QEMU virt
//! 还是龙芯实板。具体的寄存器地址和操作协议由 `platform` 层负责。

/// 请求当前 LoongArch64 平台重新启动。
pub fn restart() -> ! {
    super::platform::restart()
}

/// 请求关闭当前 LoongArch64 平台的电源。
pub fn power_off() -> ! {
    super::platform::power_off()
}

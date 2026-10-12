//! 架构抽象层：全仓库**唯一**允许出现架构分支的地方（docs/01 §3 / docs/09 §7）。
//!
//! 上层代码只能引用 `crate::arch::*`，禁止触碰 `crate::arch::riscv64::*` 这类私有路径。
//! 架构选择在 Rust 侧用 `cfg(feature = "arch-*")`，不用 `cfg(target_arch)`：
//! feature 才是构建链（Kconfig → build.mk → Cargo）的单一真相源。

#[cfg(all(feature = "arch-riscv64", feature = "arch-loongarch64"))]
compile_error!("只能启用一个架构 feature；请检查 config/.config 的 ARCH 选择");

#[cfg(not(any(feature = "arch-riscv64", feature = "arch-loongarch64")))]
compile_error!("必须启用 arch-riscv64 或 arch-loongarch64；请通过 make 构建以注入 feature");

// 后端模块保持私有：上层禁止通过 arch::riscv64::* 直接访问（docs/09 §7），
// 只允许走下面这组统一 re-export。私有模块内的 pub 项可以被 re-export，
// 但私有模块本身不能被 pub use。
#[cfg(feature = "arch-riscv64")]
mod riscv64;
#[cfg(feature = "arch-riscv64")]
pub use self::riscv64::{
    console, mm, park, power_off, restart, task, timer, trap, trap::TrapContext, ARCH_NAME,
    LINK_BASE,
};

#[cfg(feature = "arch-loongarch64")]
mod loongarch64;
#[cfg(feature = "arch-loongarch64")]
pub use self::loongarch64::{
    console, mm, park, power_off, restart, trap, trap::TrapContext, ARCH_NAME, LINK_BASE,
};

/// 公共映射权限传给架构 PTE 编码器时使用的窄接口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagePermissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub user: bool,
    pub memory_type: MemoryType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    Normal,
    Device,
}

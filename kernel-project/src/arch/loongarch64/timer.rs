//! LoongArch 本核周期定时器；仅负责 CSR 编程与中断确认。

use core::arch::asm;

/// ESTAT.IS 与 ECFG.LIE 的 TI 位，由硬件定义为 bit 11。
pub const TIMER_INTERRUPT_BIT: usize = 1 << 11;
/// 阶段性固定计数值；尚未通过平台频率校准，不作为真实时间单位。
const TIMER_INTERVAL_TICKS: usize = 1_000_000;

/// 只打开本核时钟中断；用户态恢复时由 PRMD.PIE 打开全局响应。
pub fn init() {
    let config = TIMER_INTERVAL_TICKS | 0b11;
    // SAFETY: ECFG.LIE[11] 对应 TI；CSR 指令覆写输入寄存器，故用 inout。
    unsafe {
        asm!(
            "csrxchg {enabled}, {mask}, 0x4",
            enabled = inout(reg) TIMER_INTERRUPT_BIT => _,
            mask = in(reg) TIMER_INTERRUPT_BIT,
            options(nostack)
        );
        // SAFETY: 先清除旧 TI，再用 TCFG.En|Periodic 启动周期计数。
        asm!("csrwr {one}, 0x44", one = inout(reg) 1usize => _, options(nostack));
        asm!("csrwr {config}, 0x41", config = inout(reg) config => _, options(nostack));
    }
}

/// 中断处理时确认 TI 电平，防止 ERTN 后立即重复进入同一中断。
pub fn clear_interrupt() {
    // SAFETY: 向 TICLR.CLR 写 1 只清除本核定时器中断请求。
    unsafe { asm!("csrwr {one}, 0x44", one = inout(reg) 1usize => _, options(nostack)) };
}

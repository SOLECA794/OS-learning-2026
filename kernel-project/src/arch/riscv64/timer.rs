//! S 态时钟定时器（RV64，QEMU virt + OpenSBI）。
//!
//! OpenSBI 默认把 S 态时钟中断（STI，cause 5）委托到 S 模式（MIDELEG bit5=1）；
//! 本模块通过 SBI TIME 扩展（eid=0x54494D45 "TIME"）请求 M 模式定时器，
//! 到期时以 STI 形式进入 S 模式 trap。
//!
//! 时间基数：QEMU virt 的 timebase = 10MHz（1 tick = 100ns）。当前按常量登记，
//! M2 内存发现阶段改为从 DTB 读取 timebase-frequency。

/// SBI TIME 扩展编号（"TIME"）。
const SBI_TIME_EID: usize = 0x54494d45;
const SBI_SET_TIMER_FID: usize = 0;

/// 时钟周期（tick）：10ms @ 10MHz。
const TIMER_INTERVAL: usize = 100_000;

/// 读取当前 time。
pub fn read_time() -> usize {
    let time: usize;
    // SAFETY: rdtime 无前置条件，结果写入 out 寄存器。
    unsafe { core::arch::asm!("csrr {t}, time", t = out(reg) time) };
    time
}

/// 请求 M 模式在 `stime_value` 时刻触发 S 态时钟中断。
fn sbi_set_timer(stime_value: usize) {
    // SAFETY: ecall 按 SBI ABI 传参（a7=eid, a6=fid, a0=value），由 OpenSBI 处理。
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") SBI_TIME_EID,
            in("a6") SBI_SET_TIMER_FID,
            in("a0") stime_value,
            options(nostack)
        );
    }
}

/// 使能 S 态时钟中断（sie.STIE）并设置第一次触发。
pub fn init() {
    // SAFETY: 写 sie 无前置条件；1<<5 = STIE。
    unsafe {
        core::arch::asm!(
            "csrs sie, {v}",
            v = in(reg) 1usize << 5,
            options(nostack, preserves_flags)
        );
    }
    set_next_trigger();
}

/// 重设下一次时钟触发（时钟中断 handler 中调用）。
pub fn set_next_trigger() {
    sbi_set_timer(read_time().wrapping_add(TIMER_INTERVAL));
}

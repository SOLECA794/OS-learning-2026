//! LoongArch64 控制台：QEMU virt 上的 NS16550A。
//!
//! 地址 0x1FE0_01E0，寄存器宽度 8 位、步长 1 字节（不是常见的 32 位步长）。
//! M0 阶段 boot 运行在 DA=1 物理直址下，MMIO 直接按物理地址访问，无需映射。

const UART_BASE: usize = 0x1FE0_01E0;

/// 寄存器偏移（字节）；DLAB=1 时 0/1 号寄存器改作波特率除数。
const THR: usize = 0x0; // 写：发送保持寄存器
const IER: usize = 0x1; // 中断使能
const FCR: usize = 0x2; // 写：FIFO 控制
const LCR: usize = 0x3; // 线路控制（bit7 = DLAB）
const LSR: usize = 0x5; // 线路状态（bit5 = THRE）

const LCR_8N1: u8 = 0x03;
const LCR_DLAB: u8 = 0x80;
const LSR_THRE: u8 = 0x20;

#[inline]
unsafe fn write_reg(offset: usize, value: u8) {
    unsafe { core::ptr::write_volatile((UART_BASE + offset) as *mut u8, value) }
}

#[inline]
unsafe fn read_reg(offset: usize) -> u8 {
    unsafe { core::ptr::read_volatile((UART_BASE + offset) as *const u8) }
}

/// 配置 NS16550A：8N1、关中断、开 FIFO。
///
/// 波特率除数写 1：QEMU 不按真实时钟采样串口，除数只影响真实硬件；
/// 上实板（2K1000）时需要按真实 UART 时钟重算——记入 M4 待办。
pub fn init() {
    // SAFETY: UART_BASE 是 QEMU virt 固定的串口 MMIO 地址，独占使用。
    unsafe {
        write_reg(IER, 0x00);
        write_reg(LCR, LCR_DLAB);
        write_reg(THR, 0x01); // DLL
        write_reg(IER, 0x00); // DLM
        write_reg(LCR, LCR_8N1);
        write_reg(FCR, 0x07); // 使能 FIFO 并清空
    }
}

fn putchar(ch: u8) {
    // SAFETY: 同上，MMIO 轮询，无重入（M0 只有 bootstrap hart 在用）。
    unsafe {
        while read_reg(LSR) & LSR_THRE == 0 {}
        write_reg(THR, ch);
    }
}

/// klib::log 的字节输出钩子。
///
/// # Safety
/// `ptr` 必须指向至少 `len` 字节的可读内存。调用方（klib::log）保证来自 Rust 切片。
#[no_mangle]
pub extern "C" fn kernel_arch_console_write(ptr: *const u8, len: usize) {
    // SAFETY: 契约见函数文档，唯一调用方是 klib::log::write_bytes。
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    for &b in bytes {
        putchar(b);
    }
}

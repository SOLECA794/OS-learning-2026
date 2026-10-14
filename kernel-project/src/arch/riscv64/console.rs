//! RISC-V64 控制台：走 OpenSBI 的 legacy console_putchar。
//!
//! 为什么用 legacy 而不是 DBCN（0x4442434E）：
//! legacy EID 0x01 被所有 OpenSBI 版本支持，boot 早期不需要探测扩展是否存在；
//! DBCN 需要 `sbi_probe_extension`，那是 M1 建立 SBI 封装层之后的事。
//! 退出条件：M1 引入统一 SBI 封装后改为"优先 DBCN，fallback legacy"。

use crate::klib::sync::SpinLock;

use super::platform::qemu_virt as virt;

/// RX 队列固定容量：IRQ 中不分配内存；满时丢弃新字节并计数。
const RX_CAPACITY: usize = 256;
const UART_RBR: usize = 0;
const UART_IER: usize = 1;
const UART_LSR: usize = 5;
const UART_LSR_DATA_READY: u8 = 1;
const UART_IER_RX: u8 = 1;

struct RxQueue {
    bytes: [u8; RX_CAPACITY],
    head: usize,
    len: usize,
    dropped: usize,
}

impl RxQueue {
    const fn new() -> Self {
        Self {
            bytes: [0; RX_CAPACITY],
            head: 0,
            len: 0,
            dropped: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.len == RX_CAPACITY {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        self.bytes[(self.head + self.len) % RX_CAPACITY] = byte;
        self.len += 1;
    }

    fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }
        let byte = self.bytes[self.head];
        self.head = (self.head + 1) % RX_CAPACITY;
        self.len -= 1;
        Some(byte)
    }
}

static RX: SpinLock<RxQueue> = SpinLock::new(RxQueue::new());

#[inline]
fn read8(address: usize) -> u8 {
    // SAFETY: 调用点只传入当前 QEMU virt 已映射的 UART 寄存器。
    unsafe { (address as *const u8).read_volatile() }
}

#[inline]
fn write8(address: usize, value: u8) {
    // SAFETY: 调用点只传入当前 QEMU virt 已映射的 UART 寄存器。
    unsafe { (address as *mut u8).write_volatile(value) }
}

/// legacy SBI 扩展 ID：console_putchar。
const SBI_LEGACY_CONSOLE_PUTCHAR: usize = 0x01;

/// SBI legacy 调用约定：a7 = 扩展 ID，a6 = 功能 ID（legacy 固定 0），a0 = 参数。
unsafe fn sbi_putchar(ch: u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") ch as usize => _,
            in("a6") 0usize,
            in("a7") SBI_LEGACY_CONSOLE_PUTCHAR,
            // 不声明 nomem：ecall 有外部可见副作用。
            options(nostack),
        );
    }
}

/// 初始化控制台。
///
/// RISC-V 侧为空操作：OpenSBI 的控制台在固件阶段就可用。
/// 保留此函数是为了让 `kernel::main` 的启动序列对两个架构一致（LA 侧确实需要配置 UART）。
pub fn init() {}

/// 分页激活后才允许访问 UART MMIO；这里只打开 RX 中断，不改 SBI TX 路径。
pub fn init_rx() {
    // IRQ 打开前先收走固件阶段已经到达的字节。
    drain_rx();
    write8(virt::UART0 + UART_IER, UART_IER_RX);
}

/// UART IRQ handler：只搬运硬件 FIFO，不分配、不打印、不触碰用户地址。
pub fn handle_rx_irq() {
    drain_rx();
}

fn drain_rx() {
    let mut queue = RX.lock();
    while read8(virt::UART0 + UART_LSR) & UART_LSR_DATA_READY != 0 {
        queue.push(read8(virt::UART0 + UART_RBR));
    }
}

/// 当前是否至少缓存一个输入字节；只供任务层登记等待者时重查。
pub fn has_input() -> bool {
    RX.lock().len != 0
}

/// 把已缓存字节移入调用者的内核缓冲区；无数据返回 0，不表示 EOF。
pub fn read_available(dst: &mut [u8]) -> usize {
    let mut queue = RX.lock();
    let mut read = 0;
    while read < dst.len() {
        match queue.pop() {
            Some(byte) => dst[read] = byte,
            None => break,
        }
        read += 1;
    }
    read
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
        // SAFETY: ecall 无前置条件，寄存器由 asm 显式约束。
        unsafe { sbi_putchar(b) };
    }
}

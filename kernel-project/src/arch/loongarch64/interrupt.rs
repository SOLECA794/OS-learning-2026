//! QEMU virt LS7A PCH PIC → EXTIOI → LoongArch HWI0 的 UART RX 中断路由。

use core::arch::asm;

use super::platform;

/// LoongArch ESTAT.IS/ECFG.LIE 中 HWI0 对应的外部中断位。
pub const EXTERNAL_INTERRUPT_BIT: usize = 1 << 2;
const PCH_MASK: usize = 0x20;
const PCH_EDGE: usize = 0x60;
const PCH_CLEAR: usize = 0x80;
const PCH_HTMSI_VECTOR: usize = 0x200;
const EXTIOI_NODEMAP: usize = 0x14a0;

#[inline]
fn read32(address: usize) -> u32 {
    // SAFETY: 调用点只访问已映射的 QEMU virt PCH PIC 寄存器。
    unsafe { (address as *const u32).read_volatile() }
}

#[inline]
fn write32(address: usize, value: u32) {
    // SAFETY: 调用点只访问已映射的 QEMU virt PCH PIC 寄存器。
    unsafe { (address as *mut u32).write_volatile(value) }
}

#[inline]
fn write8(address: usize, value: u8) {
    // SAFETY: PCH PIC HT vector 是字节寄存器，按字节访问避免覆盖相邻路由项。
    unsafe { (address as *mut u8).write_volatile(value) }
}

/// 向 LoongArch IOCSR 写一个 32 位控制器寄存器。
fn write_iocsr32(address: usize, value: u32) {
    // SAFETY: IOCSR 地址与寄存器宽度由 QEMU virt EXTIOI 定义；输入寄存器在汇编中只读。
    unsafe {
        asm!(
            "iocsrwr.w {value}, {address}",
            value = in(reg) value as usize,
            address = in(reg) address,
            options(nostack)
        )
    }
}

/// 配置 QEMU virt UART0 的单核外部中断通路，并最后打开 UART RX 中断。
pub fn init() {
    let irq = platform::UART_PCH_IRQ;
    let bit = 1u32 << irq;
    let pch = platform::PCH_PIC_BASE;

    // UART IRQ2 使用 level-high；清理旧 pending，再取消 PIC mask。
    write32(pch + PCH_EDGE, read32(pch + PCH_EDGE) & !bit);
    write32(pch + PCH_CLEAR, bit);
    write8(pch + PCH_HTMSI_VECTOR + irq, irq as u8);
    write32(pch + PCH_MASK, read32(pch + PCH_MASK) & !bit);

    // QEMU virt 单核测试将该 EXTIOI 中断路由到 node 0 / CPU HWI0。
    write_iocsr32(EXTIOI_NODEMAP, 0);
    write_iocsr32(platform::EXTIOI_IPMAP_BASE, 0);
    write_iocsr32(platform::EXTIOI_ENABLE_BASE, bit);

    // ECFG.LIE[2] 使能 HWI0；全局 IE 由用户返回现场或 idle_once 控制。
    unsafe {
        asm!(
            "csrxchg {enabled}, {mask}, 0x4",
            enabled = inout(reg) EXTERNAL_INTERRUPT_BIT => _,
            mask = in(reg) EXTERNAL_INTERRUPT_BIT,
            options(nostack)
        );
    }
    super::console::init_rx();
}

/// 消费 UART FIFO 并确认 EXTIOI/PCH PIC；当前只开放 UART0 一个外部源。
pub fn handle_external() -> bool {
    super::console::handle_rx_irq();
    let bit = 1u32 << platform::UART_PCH_IRQ;
    write_iocsr32(platform::EXTIOI_ISR_BASE, bit);
    write32(platform::PCH_PIC_BASE + PCH_CLEAR, bit);
    true
}

/// 无就绪任务但有控制台读者时，短暂打开 CRMD.IE 后进入 idle 等待中断。
pub fn idle_once() {
    // SAFETY: 调度器已释放锁；返回后关闭内核中断，避免普通内核代码被异步抢占。
    unsafe {
        asm!(
            "csrxchg {enabled}, {mask}, 0x0",
            "idle 0",
            "csrxchg $zero, {mask}, 0x0",
            enabled = inout(reg) 0b100usize => _,
            mask = in(reg) 0b100usize,
            options(nostack)
        );
    }
}

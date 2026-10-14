//! QEMU virt PLIC 的最小 UART RX 外部中断路由（bootstrap hart / S context）。

use core::arch::asm;
use core::sync::atomic::{AtomicUsize, Ordering};

use super::platform::qemu_virt as virt;

// 唯一 bootstrap hart 的 PLIC context；init 写入后，IRQ handler 只读取。
static BOOT_HART: AtomicUsize = AtomicUsize::new(usize::MAX);

fn read32(address: usize) -> u32 {
    // SAFETY: 调用点只传入已映射的 PLIC 32 位寄存器。
    unsafe { (address as *const u32).read_volatile() }
}

fn write32(address: usize, value: u32) {
    // SAFETY: 调用点只传入已映射的 PLIC 32 位寄存器。
    unsafe { (address as *mut u32).write_volatile(value) }
}

/// 页表激活后、进入用户态前调用。只使能 UART0 一个外部中断源。
pub fn init(hartid: usize) {
    assert!(
        hartid < virt::HART_COUNT,
        "bootstrap hart outside boot stack range"
    );
    BOOT_HART.store(hartid, Ordering::Release);
    super::console::init_rx();
    write32(virt::PLIC + 4 * virt::UART0_IRQ as usize, 1);
    write32(virt::plic_s_threshold(hartid), 0);
    write32(virt::plic_s_enable(hartid), 1u32 << virt::UART0_IRQ);
    // SAFETY: SIE bit9=SEIE；trap handler 已能认出 supervisor external interrupt。
    unsafe {
        asm!("csrs sie, {value}", value = in(reg) 1usize << 9, options(nostack, preserves_flags))
    };
}

/// claim → 处理 UART RX → complete。未知来源不得冒充处理成功。
pub fn handle_external() -> bool {
    let hartid = BOOT_HART.load(Ordering::Acquire);
    assert!(hartid < virt::HART_COUNT, "PLIC IRQ before bootstrap init");
    let claim = virt::plic_s_claim(hartid);
    let irq = read32(claim);
    if irq == 0 {
        return true; // spurious：无 pending source
    }
    if irq == virt::UART0_IRQ {
        super::console::handle_rx_irq();
        write32(claim, irq);
        return true;
    }
    write32(claim, irq);
    false
}

/// 队列空、没有就绪任务时短暂允许 S 态中断并等待下一次事件。
/// 定时器保持周期触发，覆盖「检查 ready 后、执行 wfi 前」的中断竞态。
pub fn idle_once() {
    // SAFETY: 单 bootstrap hart；调用方不持任何锁，退出时恢复内核态 SIE=0。
    unsafe {
        asm!(
            "csrsi sstatus, 2",
            "wfi",
            "csrci sstatus, 2",
            options(nostack)
        );
    }
}

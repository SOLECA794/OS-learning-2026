//! 架构无关的内存管理核心。
//!
//! 当前阶段建立地址类型、页帧分配器和内核 MemorySet，并激活内核页表。

use crate::klib::sync::SpinLock;

pub(crate) mod address;
pub(crate) mod frame_allocator;
pub(crate) mod heap_allocator;
pub(crate) mod kernel_space;
pub(crate) mod kernel_stack;
pub(crate) mod memory_set;
pub(crate) mod page_table;

/// 内核当前激活地址空间的唯一所有权位置。
static KERNEL_SPACE: SpinLock<Option<memory_set::MemorySet>> = SpinLock::new(None);

/// 读取并校验链接器给出的内核映像布局。
///
/// 初始化页帧分配器、构造内核页表并通过架构后端激活分页。
pub(crate) fn init() {
    let layout = kernel_space::KernelImageLayout::current();
    let memory_bytes = configured_memory_bytes();
    let (memory_start, memory_end) = crate::arch::mm::physical_memory_range(memory_bytes)
        .expect("configured QEMU memory is not a supported contiguous RAM range");
    assert!(
        memory_start <= layout.text.start().value() && layout.end.value() <= memory_end,
        "kernel image lies outside configured physical RAM"
    );
    let first_free = layout
        .end
        .ceil()
        .expect("kernel end address cannot be rounded to a page");
    let memory_end_ppn = address::PhysAddr::new(memory_end).floor();
    frame_allocator::init(
        address::PhysPageNum::new(first_free.value()),
        memory_end_ppn,
    )
    .expect("physical frame allocator initialization failed");
    let kernel_space =
        memory_set::MemorySet::new_kernel(layout, address::VirtAddr::new(memory_end))
            .expect("kernel MemorySet construction failed");
    let root_ppn = kernel_space.root_ppn();
    let area_count = kernel_space.area_count();
    crate::log_info!(
        "kernel image: text={:?} rodata={:?} data={:?} bss={:?} boot_stack={:?} end={:?}",
        layout.text,
        layout.rodata,
        layout.data,
        layout.bss,
        layout.boot_stack,
        layout.end,
    );
    crate::log_info!(
        "mm: free frames=[{:?}, {:?}) allocated={} root={:?} areas={} activating paging",
        first_free,
        memory_end_ppn,
        frame_allocator::allocated_count(),
        root_ppn,
        area_count,
    );
    // SAFETY: new_kernel 已身份映射当前代码、栈、BSS、空闲 RAM 与必需 MMIO。
    unsafe { crate::arch::mm::activate(root_ppn.value()) };
    *KERNEL_SPACE.lock() = Some(kernel_space);
    crate::log_info!("mm: kernel page table active");
    heap_allocator::init().expect("kernel heap initialization failed");
    heap_allocator::self_test();
    memory_set::self_test();
    frame_allocator::self_test();
    let stats = heap_allocator::stats();
    crate::log_info!(
        "mm: kernel heap active size={} free={} allocations={}",
        stats.total_bytes,
        stats.free_bytes,
        stats.allocations,
    );
}

/// 解析构建配置中的 QEMU RAM MiB 数并转换为字节。
pub(crate) fn configured_memory_bytes() -> usize {
    let mut megabytes = 0usize;
    for byte in env!("KERNEL_MEMORY_MB").bytes() {
        assert!(byte.is_ascii_digit(), "KERNEL_MEMORY_MB must be decimal");
        megabytes = megabytes
            .checked_mul(10)
            .and_then(|value| value.checked_add((byte - b'0') as usize))
            .expect("KERNEL_MEMORY_MB overflow");
    }
    assert!(megabytes > 0, "KERNEL_MEMORY_MB must be greater than zero");
    megabytes
        .checked_mul(1024 * 1024)
        .expect("configured memory byte count overflow")
}

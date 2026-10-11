//! 任务内核栈的内存所有权。
//!
//! 当前内核页表对可用 RAM 使用身份映射，因此连续物理页同时提供连续内核虚拟地址。
//! 后续切换到高半内核后，本模块应改为把离散页映射进带 guard page 的栈虚拟区；
//! 调度器和任务对象仍只依赖这里暴露的 `bottom` / `top`，不感知映射策略。

use super::address::PAGE_SIZE;
use super::frame_allocator::{self, FrameAllocError, FrameBlock};

/// 起步阶段每个任务使用 16 KiB 内核栈。
pub const KERNEL_STACK_PAGES: usize = 4;
/// 单个任务内核栈的字节数。
pub const KERNEL_STACK_SIZE: usize = KERNEL_STACK_PAGES * PAGE_SIZE;

/// 一段向低地址增长、16 字节对齐的内核栈。
pub struct KernelStack {
    frames: FrameBlock,
}

impl KernelStack {
    /// 分配一段连续且已清零的内核栈内存。
    pub fn new() -> Result<Self, FrameAllocError> {
        Ok(Self {
            frames: frame_allocator::allocate_contiguous(KERNEL_STACK_PAGES)?,
        })
    }

    /// 返回栈映射的最低虚拟地址。
    pub fn bottom(&self) -> usize {
        crate::arch::mm::phys_to_virt(self.frames.start().start_address().value())
    }

    /// 返回向下增长栈的初始栈指针位置。
    pub fn top(&self) -> usize {
        let top = self
            .bottom()
            .checked_add(KERNEL_STACK_SIZE)
            .expect("kernel stack address overflow");
        assert_eq!(top & 0xf, 0, "kernel stack top must be 16-byte aligned");
        top
    }

    /// 返回内核栈固定容量，单位为字节。
    pub const fn size(&self) -> usize {
        KERNEL_STACK_SIZE
    }
}

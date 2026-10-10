//! 当前内核 ELF 映像的地址布局。

use super::address::{AddressRange, VirtAddr};

/// 链接器给出的内核各段半开地址范围。
#[derive(Debug, Clone, Copy)]
pub struct KernelImageLayout {
    /// 可读、可执行的指令段。
    pub text: AddressRange<VirtAddr>,
    /// 只读数据段。
    pub rodata: AddressRange<VirtAddr>,
    /// 已初始化的可读写数据段。
    pub data: AddressRange<VirtAddr>,
    /// 启动时清零的可读写 BSS 段。
    pub bss: AddressRange<VirtAddr>,
    /// bootstrap core 使用的早期内核栈。
    pub boot_stack: AddressRange<VirtAddr>,
    /// 整个内核静态映像不包含的结束地址。
    pub end: VirtAddr,
}

impl KernelImageLayout {
    /// 从链接脚本符号读取当前 ELF 映像布局并校验段顺序。
    pub fn current() -> Self {
        unsafe extern "C" {
            static __text_start: u8;
            static __text_end: u8;
            static __rodata_start: u8;
            static __rodata_end: u8;
            static __data_start: u8;
            static __data_end: u8;
            static __bss_start: u8;
            static __bss_end: u8;
            static __boot_stack_bottom: u8;
            static __boot_stack_top: u8;
            static __kernel_end: u8;
        }

        // 链接符号只提供地址，本身不代表需要读取的 Rust 对象。
        let address = |symbol: *const u8| VirtAddr::new(symbol as usize);
        let range = |start, end| {
            AddressRange::new(address(start), address(end))
                .expect("linker emitted a reversed kernel section")
        };
        let layout = Self {
            text: range(&raw const __text_start, &raw const __text_end),
            rodata: range(&raw const __rodata_start, &raw const __rodata_end),
            data: range(&raw const __data_start, &raw const __data_end),
            bss: range(&raw const __bss_start, &raw const __bss_end),
            boot_stack: range(&raw const __boot_stack_bottom, &raw const __boot_stack_top),
            end: address(&raw const __kernel_end),
        };
        layout.assert_ordered();
        layout
    }

    /// 验证链接脚本没有生成重叠或逆序的内核段。
    fn assert_ordered(&self) {
        assert!(self.text.end() <= self.rodata.start());
        assert!(self.rodata.end() <= self.data.start());
        assert!(self.data.end() <= self.bss.start());
        assert!(self.bss.end() <= self.boot_stack.start());
        assert!(self.boot_stack.end() <= self.end);
    }
}

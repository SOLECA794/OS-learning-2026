//! 物理/虚拟地址、页号和地址范围的强类型封装。
//!
//! 内存管理代码需要同时处理物理地址、虚拟地址、物理页号和虚拟页号。它们在机器上
//! 最终都是整数，但语义完全不同；使用独立类型可以让 Rust 在编译期阻止误用。
//! 本模块只负责表示、对齐和范围运算，不负责分配页帧或修改页表。

use core::fmt;

/// 页内偏移占用的位数；4 KiB 页面对应 12 位。
pub const PAGE_SHIFT: usize = 12;
/// 内核基础页面大小：`2^12 = 4096` 字节。
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
/// 用于提取页内偏移或检查页对齐的低位掩码。
const PAGE_MASK: usize = PAGE_SIZE - 1;

/// 地址和页号运算中可报告的错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressError {
    /// 调用者要求整页地址，但地址仍带有页内偏移。
    Unaligned,
    /// 加法或向上取整超过了 `usize` 能表示的范围。
    Overflow,
    /// 半开区间的结束位置小于开始位置。
    InvalidRange,
}

/// 为物理/虚拟两组地址类型生成完全相同的安全运算。
///
/// `$name` 表示字节地址类型，`$page` 表示对应页号类型。使用宏是为了确保两组类型
/// 行为一致，同时仍让编译器把它们视为互不兼容的类型。
macro_rules! define_address {
    ($name:ident, $page:ident) => {
        #[doc = concat!("按字节计数的强类型地址 `", stringify!($name), "`。")]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(usize);

        impl $name {
            /// 从原始整数构造地址；不隐式要求页对齐。
            pub const fn new(value: usize) -> Self {
                Self(value)
            }

            /// 取出底层整数，供寄存器、页表项或指针边界使用。
            pub const fn value(self) -> usize {
                self.0
            }

            /// 判断地址是否位于页面起点。
            pub const fn is_page_aligned(self) -> bool {
                self.0 & PAGE_MASK == 0
            }

            /// 向下取整到地址所在页面。
            ///
            /// 该操作丢弃低 `PAGE_SHIFT` 位，即页内偏移。
            pub const fn floor(self) -> $page {
                $page(self.0 >> PAGE_SHIFT)
            }

            /// 向上取整到能够覆盖该地址的页号。
            ///
            /// 已对齐地址保持原页号；未对齐地址进入下一页。接近 `usize::MAX` 时，
            /// 加上掩码可能溢出，因此返回 `Result`。
            pub fn ceil(self) -> Result<$page, AddressError> {
                let rounded = self
                    .0
                    .checked_add(PAGE_MASK)
                    .ok_or(AddressError::Overflow)?;
                Ok($page(rounded >> PAGE_SHIFT))
            }

            /// 在不允许整数回绕的前提下增加字节偏移。
            pub fn checked_add(self, bytes: usize) -> Result<Self, AddressError> {
                self.0
                    .checked_add(bytes)
                    .map(Self)
                    .ok_or(AddressError::Overflow)
            }

            /// 把已经页对齐的地址转换为页号。
            ///
            /// 与 `floor` 不同，这个接口不会静默丢弃页内偏移。
            pub fn page_number(self) -> Result<$page, AddressError> {
                if self.is_page_aligned() {
                    Ok(self.floor())
                } else {
                    Err(AddressError::Unaligned)
                }
            }
        }

        /// 地址在诊断日志中统一以十六进制显示。
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:#x}", self.0)
            }
        }

        #[doc = concat!("以页面为单位的强类型页号 `", stringify!($page), "`。")]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $page(usize);

        impl $page {
            /// 从原始页号构造强类型值。
            pub const fn new(value: usize) -> Self {
                Self(value)
            }

            /// 取出底层页号，用于页表索引或 PTE 编码。
            pub const fn value(self) -> usize {
                self.0
            }

            /// 返回该页面第一个字节的地址。
            pub const fn start_address(self) -> $name {
                $name(self.0 << PAGE_SHIFT)
            }

            /// 在不允许整数回绕的前提下增加页面数量。
            pub fn checked_add(self, pages: usize) -> Result<Self, AddressError> {
                self.0
                    .checked_add(pages)
                    .map(Self)
                    .ok_or(AddressError::Overflow)
            }
        }

        /// 页号在诊断日志中统一以十六进制显示。
        impl fmt::Debug for $page {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:#x}", self.0)
            }
        }
    };
}

// 物理地址只描述实际 RAM/MMIO 位置；物理页号由页帧分配器和 PTE 使用。
define_address!(PhysAddr, PhysPageNum);
// 虚拟地址是 CPU 开启分页后看到的地址；虚拟页号用于页表遍历。
define_address!(VirtAddr, VirtPageNum);

/// 虚拟字节地址的半开区间 `[start, end)`。
///
/// 半开区间不包含 `end`，因此长度始终是 `end - start`，相邻区域也能共享边界而不重叠。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressRange<A> {
    start: A,
    end: A,
}

impl AddressRange<VirtAddr> {
    /// 构造虚拟地址范围，拒绝反向区间；允许表示空区间。
    pub fn new(start: VirtAddr, end: VirtAddr) -> Result<Self, AddressError> {
        if start <= end {
            Ok(Self { start, end })
        } else {
            Err(AddressError::InvalidRange)
        }
    }

    /// 返回区间包含的第一个地址。
    pub const fn start(self) -> VirtAddr {
        self.start
    }

    /// 返回区间不包含的结束地址。
    pub const fn end(self) -> VirtAddr {
        self.end
    }

    /// 返回区间的字节长度。
    pub const fn len(self) -> usize {
        self.end.value() - self.start.value()
    }

    /// 判断区间是否不包含任何字节。
    pub const fn is_empty(self) -> bool {
        self.start.value() == self.end.value()
    }
}

/// 虚拟页号的半开区间 `[start, end)`。
///
/// `MemorySet` 使用该类型逐页建立或撤销映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VpnRange {
    start: VirtPageNum,
    end: VirtPageNum,
}

impl VpnRange {
    /// 构造页号范围，拒绝结束页小于起始页的反向区间。
    pub fn new(start: VirtPageNum, end: VirtPageNum) -> Result<Self, AddressError> {
        if start <= end {
            Ok(Self { start, end })
        } else {
            Err(AddressError::InvalidRange)
        }
    }

    /// 取得覆盖整个字节范围的最小完整页面范围。
    ///
    /// 起点向下取整，终点向上取整。例如 `[0x1234, 0x4567)` 会覆盖页面
    /// `[0x1000, 0x5000)`。
    pub fn covering(range: AddressRange<VirtAddr>) -> Result<Self, AddressError> {
        Self::new(range.start().floor(), range.end().ceil()?)
    }

    /// 返回范围包含的第一个虚拟页号。
    pub const fn start(self) -> VirtPageNum {
        self.start
    }

    /// 返回范围不包含的结束虚拟页号。
    pub const fn end(self) -> VirtPageNum {
        self.end
    }

    /// 判断范围是否不包含任何页面。
    pub const fn is_empty(self) -> bool {
        self.start.value() == self.end.value()
    }

    /// 判断两个半开页范围是否共享至少一个页面。
    ///
    /// 相邻范围（一个的 `end` 等于另一个的 `start`）不算重叠。
    pub const fn overlaps(self, other: Self) -> bool {
        self.start.value() < other.end.value() && other.start.value() < self.end.value()
    }
}

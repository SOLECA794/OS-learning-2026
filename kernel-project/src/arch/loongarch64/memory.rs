//! LoongArch 裸机内存填充后端。
//!
//! 当前工具链的 compiler-builtins `memset` 可能生成 LSX 指令，而比赛 QEMU 的
//! LoongArch CPU 未向内核提供该扩展。此实现只用基础字节存储指令。

/// 填充 `dst` 指向的 `len` 字节并返回原始指针，遵循 C `memset` ABI。
///
/// # Safety
/// 调用方必须保证目标范围可写；编译器生成的填充调用满足这一条件。
#[no_mangle]
pub unsafe extern "C" fn memset(dst: *mut u8, value: i32, len: usize) -> *mut u8 {
    let byte = value as u8;
    // volatile 阻止编译器把这个循环再次降成 memset 调用或向量指令。
    for offset in 0..len {
        // SAFETY: 可写范围由调用方保证，offset 严格小于 len。
        unsafe { dst.add(offset).write_volatile(byte) };
    }
    dst
}

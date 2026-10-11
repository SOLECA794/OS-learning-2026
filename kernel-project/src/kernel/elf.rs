//! 最小 ELF64 解析器（M1.5：仅支持静态 RV64 ET_EXEC）。
//!
//! 治理裁决：最小依赖，禁止引入第三方 crate；只解析启动用户程序所需的字段。

use alloc::vec::Vec;
use core::mem::size_of;

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const EM_RISCV: u16 = 243;
const ET_EXEC: u16 = 2;
const PT_LOAD: u32 = 1;

/// ELF 解析错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    BadMagic,
    BadClass,
    BadMachine,
    BadType,
    Truncated,
    Overflow,
    BadSegment,
    NoLoadSegments,
}

/// 段可执行标志（对应 ELF p_flags 的 PF_X/PF_W/PF_R）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentFlags {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

/// 一个 LOAD 段（已校验，vaddr/mem_bytes/file_offset/file_bytes 均不越界）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadSegment {
    pub vaddr: usize,
    pub mem_bytes: usize,
    pub file_offset: usize,
    pub file_bytes: usize,
    pub flags: SegmentFlags,
}

/// 解析结果：入口地址 + LOAD 段列表。
#[derive(Debug, Clone)]
pub struct Elf {
    pub entry: usize,
    pub segments: Vec<LoadSegment>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

/// 解析 ELF 字节流。
pub fn parse(data: &[u8]) -> Result<Elf, ElfError> {
    if data.len() < size_of::<Elf64Ehdr>() {
        return Err(ElfError::Truncated);
    }
    // SAFETY: 长度已校验；结构体是 repr(C) 的只读视图。
    let ehdr = unsafe { &*(data.as_ptr() as *const Elf64Ehdr) };
    if ehdr.e_ident[0..4] != ELF_MAGIC {
        return Err(ElfError::BadMagic);
    }
    if ehdr.e_ident[4] != ELFCLASS64 {
        return Err(ElfError::BadClass);
    }
    if ehdr.e_machine != EM_RISCV {
        return Err(ElfError::BadMachine);
    }
    if ehdr.e_type != ET_EXEC {
        return Err(ElfError::BadType);
    }
    let phoff = ehdr.e_phoff as usize;
    let phentsize = ehdr.e_phentsize as usize;
    let phnum = ehdr.e_phnum as usize;
    if phentsize < size_of::<Elf64Phdr>() {
        return Err(ElfError::BadSegment);
    }
    let table_end = phoff
        .checked_add(phentsize.saturating_mul(phnum))
        .ok_or(ElfError::Overflow)?;
    if table_end > data.len() {
        return Err(ElfError::Truncated);
    }

    let mut segments = Vec::new();
    for i in 0..phnum {
        let off = phoff + i * phentsize;
        // SAFETY: table_end 已校验在 data 内。
        let phdr = unsafe { &*(data.as_ptr().add(off) as *const Elf64Phdr) };
        if phdr.p_type != PT_LOAD {
            continue;
        }
        let vaddr = phdr.p_vaddr as usize;
        let memsz = phdr.p_memsz as usize;
        let filesz = phdr.p_filesz as usize;
        let foff = phdr.p_offset as usize;
        if filesz > memsz {
            return Err(ElfError::BadSegment);
        }
        if foff
            .checked_add(filesz)
            .map_or(true, |end| end > data.len())
        {
            return Err(ElfError::Truncated);
        }
        if vaddr.checked_add(memsz).is_none() {
            return Err(ElfError::Overflow);
        }
        let flags = SegmentFlags {
            read: phdr.p_flags & 4 != 0,
            write: phdr.p_flags & 2 != 0,
            execute: phdr.p_flags & 1 != 0,
        };
        segments.push(LoadSegment {
            vaddr,
            mem_bytes: memsz,
            file_offset: foff,
            file_bytes: filesz,
            flags,
        });
    }
    if segments.is_empty() {
        return Err(ElfError::NoLoadSegments);
    }
    Ok(Elf {
        entry: ehdr.e_entry as usize,
        segments,
    })
}

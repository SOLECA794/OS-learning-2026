//! 最小命名只读程序源（V2 §9：程序来源要在 exec 之前可用）。
//!
//! G4 阶段以静态注册表提供 exec(path) 的程序来源：名字 → 内嵌 ELF 字节。
//! G5 由 ramfs + 路径查找接管（本模块保留为 boot 程序源回落）。
//! 路径按最简规则解析：去掉前导 '/' 后查注册表；不存在返回 None（→ ENOENT）。

pub struct Program {
    pub name: &'static str,
    pub elf: &'static [u8],
}

/// 程序注册表：uapp/uapp2 为正常用户程序；bad 为故意损坏的 ELF
/// （exec 失败保留旧进程验收用例）。
pub static PROGRAMS: &[Program] = &[
    Program {
        name: "uapp",
        elf: include_bytes!("../../../build/user/uapp.elf"),
    },
    Program {
        name: "uapp2",
        elf: include_bytes!("../../../build/user/uapp2.elf"),
    },
    // 64 字节 0xff：魔数错误 → ElfError::BadMagic → exec 返回 -ENOEXEC。
    Program {
        name: "bad",
        elf: &[0xff; 64],
    },
];

/// 按路径查程序源。路径可为 "/name" 或 "name"。
pub fn lookup(path: &str) -> Option<&'static [u8]> {
    let name = path.trim_start_matches('/');
    PROGRAMS.iter().find(|p| p.name == name).map(|p| p.elf)
}

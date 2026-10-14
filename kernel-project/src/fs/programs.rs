//! 最小命名只读程序源（V2 §9：程序来源要在 exec 之前可用）。
//!
//! 仅供 boot 填充 ramfs；运行时 exec 和 open 均经 VFS 查找已发布的节点。

pub struct Program {
    pub name: &'static str,
    pub elf: &'static [u8],
}

/// 程序注册表：uapp/uapp2 为正常用户程序；bad 为故意损坏的 ELF
/// （exec 失败保留旧进程验收用例）。
pub static PROGRAMS: &[Program] = &[
    // VFS 定向测试源：由用户程序复制到子目录后 exec，验证 argv 和退出码。
    #[cfg(feature = "uapp-vfs")]
    Program {
        name: "vfs-exec",
        elf: include_bytes!("../../build/user/vfs-exec.elf"),
    },
    Program {
        name: "uapp",
        elf: include_bytes!("../../build/user/uapp.elf"),
    },
    Program {
        name: "uapp2",
        elf: include_bytes!("../../build/user/uapp2.elf"),
    },
    // 64 字节 0xff：魔数错误 → ElfError::BadMagic → exec 返回 -ENOEXEC。
    Program {
        name: "bad",
        elf: &[0xff; 64],
    },
];

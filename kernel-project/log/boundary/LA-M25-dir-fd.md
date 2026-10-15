# LoongArch M2.5 目录与元数据边界

- 日期：2026-10-15；分类：小迭代；状态：固定范围运行验收通过。
- 目标：开放现有公共 `mkdirat/getdents64/fstat/dup2` 给 LoongArch，并检查目录 FD、记录、stat 字段及 FD 复制。
- 非目标：不修改公共 VFS/FD owner，不宣称管道、时钟抢占或高地址用户栈已完成。
- Owner：目录与文件描述归 `kernel/fs`；FD 槽归 `kernel/task`；Linux UAPI 编码归 `kernel/syscall`。
- Handoff：LoongArch trap 按现有六参 ABI 交给共享 syscall 分发；架构层仅选择测试 ELF。
- Failure：坏 FD 返回 EBADF；非目录枚举返回 ENOTDIR；文件写回和游标沿用公共实现。
- Cleanup：测试在退出前关闭文件与目录 FD；进程退出由 task 统一释放剩余引用。
- 保护契约：RISC-V syscall 路径、LoongArch 既有文件/进程/exec 回归、编号与结构布局不变。
- 验收：原生 LoongArch 目录程序成功，已有 LoongArch 文件/FD 测试及 RISC-V VFS 不退化。
- 停止条件：若需要修改公共 VFS owner 或 Linux ABI 布局，先另立边界。

agent-run：比赛镜像 `test-la-dir RUN_ID=20261015-la-dir-a`、
`test-la-fd-ops RUN_ID=20261015-la-dir-fd-regress` 与 RISC-V
`test-vfs RUN_ID=20261015-la-dir-rv-vfs` 均通过。

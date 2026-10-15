# LoongArch M2.5 文件 FD 第二批边界

- 日期：2026-10-15；分类：小迭代；状态：固定范围运行验收通过。
- 目标：LoongArch 复用公共 `write/lseek/pread64/pwrite64/dup` 文件路径，验证内容与共享游标。
- 非目标：不宣称完整 FD ABI、时钟抢占、高地址用户栈或控制台写入 FD 化完成。
- Owner：FD 槽归 `kernel/task`，文件内容和游标归 `kernel/fs`，系统调用参数归 `kernel/syscall`；架构层只选择原生测试 ELF。
- Handoff：普通文件写入经过用户缓冲拷贝，再调用公共打开描述；stdout/stderr 保留 LoongArch 已验收桥。
- Failure：坏 FD 返回 EBADF，坏地址返回 EFAULT；定位 IO 不推进共享游标。
- Cleanup：两个 dup FD 各自 close，最后一个引用由现有 Arc 路径释放。
- 保护契约：Linux syscall 编号/参数、RISC-V 文件 ABI、LoongArch 原控制台输出不变。
- 验收：比赛镜像中 `test-la-fd-ops` 成功；原 `test-la-file` 和 RISC-V `test-vfs` 不退化。
- 停止条件：若需要改 VFS owner 或改变原 console write 行为，先另立边界。

agent-run：比赛镜像 `test-la-fd-ops RUN_ID=20261015-la-fd-ops-a`、
`test-la-file RUN_ID=20261015-la-fd-ops-file-regress` 和 RISC-V
`test-vfs RUN_ID=20261015-la-fd-ops-rv-vfs` 均通过。

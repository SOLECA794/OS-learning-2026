# LoongArch 用户态堆与匿名映射边界

- 日期：2026-10-16；分类：小迭代；状态：固定范围运行验收通过。
- 目标：LoongArch 复用公共 MemorySet 的 `brk`、匿名 `mmap`、`munmap` syscall，验证用户页读写与解除映射。
- 非目标：不实现文件映射、mprotect、完整 mmap flags 或堆页 COW 故障注入。
- Owner：页表与映射区属于 `kernel/mm/MemorySet`；参数/errno 属于 `kernel/syscall`；架构层只选择原生用户 ELF。
- Handoff：syscall 只经当前任务地址空间的窄接口调用 MemorySet，不在架构层新建映射状态。
- Failure：分配失败维持现有 ENOMEM，非法 munmap 返回 EINVAL；不改变 RISC-V 已有语义。
- Cleanup：munmap 由 MemorySet 回收页，任务退出释放其地址空间。
- 保护契约：RISC-V 用户内存调用与 LoongArch 已验收进程、FD、时钟路径不变。
- 验收：LoongArch 原生用户程序验证 brk 扩展/收缩、mmap 写读/munmap；RISC-V brk/mmap 回归不退化。
- 停止条件：若需改 MemorySet owner 或扩展 Linux mmap ABI，另立边界。

agent-run：比赛镜像 LoongArch `test-la-memory RUN_ID=20261016-la-memory-b`、
RISC-V `test-brk RUN_ID=20261016-la-memory-rv-brk` 和
`test-mmap RUN_ID=20261016-la-memory-rv-mmap` 均通过。

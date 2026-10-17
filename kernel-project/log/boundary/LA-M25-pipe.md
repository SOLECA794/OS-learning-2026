# LoongArch M2.5 pipe 与重定向边界

- 日期：2026-10-16；分类：小迭代；状态：固定范围运行验收通过。
- 目标：LoongArch 复用公共 `pipe`，支持经 `dup2` 重定向的 fd 1 写入；修复公共 pipe 创建时写回用户 FD 对失败后的泄漏。
- 非目标：不实现阻塞管道、SIGPIPE、时钟抢占或高地址用户栈；不宣称全量 Linux FD ABI。
- Owner：管道缓冲归 `kernel/fs`，FD 槽归 `kernel/task`，用户指针与回滚归 `kernel/syscall`；架构层仅负责原生 ELF 测试。
- Handoff：仅当 fd 1/2 仍指向控制台输出时使用 LoongArch 已有桥；重定向后交给公共 FD 写路径。
- Failure：用户指针无效时返回 EFAULT，不能留下半创建 FD；坏 FD 返回 EBADF。
- Cleanup：第二个 FD 分配或用户写回失败时关闭已分配槽；正常路径由 close/exit 释放引用。
- 保护契约：已有 LoongArch 串口输出、RISC-V pipe/dup2 行为、Linux syscall 编号与用户 FD 数组布局不变。
- 验收：原生 LoongArch pipe 写读与 fd 1 重定向测试通过，既有 LA FD/目录及 RISC-V pipe 回归不退化。
- 停止条件：若需改变公共管道阻塞语义或 owner，先另立 RFC。

agent-run：比赛镜像 `test-la-pipe RUN_ID=20261016-la-pipe-a`、
`test-la-dir RUN_ID=20261016-la-pipe-dir-regress` 与 RISC-V
`test-pipe RUN_ID=20261016-la-pipe-rv-regress` 均通过。

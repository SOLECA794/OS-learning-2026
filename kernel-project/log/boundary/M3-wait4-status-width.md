# M3 `wait4` 状态字段 ABI 边界

- 日期：2026-10-14；分类：RFC 的单次 ABI cutover。
- 目标：RISC-V `wait4(pid, wstatus, options, rusage)` 成功时仅向 `wstatus` 指向的 32 位 `int` 写入退出状态；紧邻的 4 字节保持原值。
- 非目标：PID 组、完整 options/rusage、信号等待状态、LoongArch 用户态或 `openat` ABI。
- Owner：`kernel/syscall` 负责 Linux 用户 ABI 编码和拷贝，`kernel/task` 继续独占僵尸回收。
- Handoff：task 返回退出码快照；syscall 写入 4 字节成功后调用 `reap_child`。
- Failure：用户地址不完整可写则返回 EFAULT，子任务保持僵尸以供重试；不允许只写部分状态。
- Cleanup：成功后 task 一次性释放子地址空间、内核栈与文件引用；失败不回收。
- 受保护契约：系统调用号 260、四寄存器参数位置、`WNOHANG` 已有行为、退出码 `((code & 0xff) << 8)`、EFAULT 重试和一次回收语义。
- 明确替换：项目旧 `usize`（8 字节）状态写回改为 Linux `int`（4 字节）。同步迁移仓库内所有调用方；不保留会覆盖相邻内存的旧行为桥。
- 验收：用户态 4 字节状态与后续 canary 相邻，wait 后退出码正确且 canary 原样；错误地址两次 EFAULT 后可成功 wait；进程、COW、VFS、shell、console 回归通过。
- 验证方式：比赛容器内 `make ARCH=riscv64 run-cow`、`test-process`、`test-vfs`、`test-shell`、`test-console`，LoongArch 独立临时副本构建。
- 停止条件：发现仓库外受支持的旧 8 字节调用方、需要改变系统调用号/owner/调度协议，或测试无法证明写入宽度。

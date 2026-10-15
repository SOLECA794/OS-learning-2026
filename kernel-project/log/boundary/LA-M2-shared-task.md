# LoongArch 接入共享 task 的实施边界

- 日期：2026-10-15；分类：RFC；状态：单任务共享 TCB 路径已运行验证。
- **目标**：LoongArch 首任务由现有 `kernel/task/mod.rs` 的 TCB、调度状态及地址空间 owner 持有，保留单任务用户态行为。
- **非目标**：不启用 LoongArch 时钟中断、抢占、fork/exec/wait、文件系统调用或 COW；不修改 RISC-V 现有调度语义。
- **Owner**：共享 `kernel::task` 管理任务生命周期；`arch::task` 只负责寄存器切换，`arch::trap` 只负责 trap 现场。
- **Handoff**：首任务经 `create_task`、`launch_first` 发布，再激活页表并恢复用户现场；用户指针读取使用 `with_current_space`。
- **Failure**：创建失败 panic；用户指针失败 EFAULT；最后任务退出时保留当前栈并停驻。
- **Cleanup**：验证后删除 `loongarch_single.rs` 临时桥；任务退出后的回收仍待多任务调度实现。
- **保护契约**：LoongArch trap ABI、`write/exit` 行为、RISC-V task/FD/console 行为不变。
- **验收**：LoongArch `test-user`、`test-la-switch` 有界 QEMU 通过；RISC-V `make check` 和 `make fmt-check` 通过。
- **停止条件**：若需改 trap 现场布局、页表切换协议或打开 LA 中断，停止并另立 RFC。

路线：CKPT 1 编译共享 task 的 LA 单任务路径；CKPT 2 运行双架构定向回归后移除临时桥。

agent-run：LoongArch `test-user`、`test-la-switch`，RISC-V `test-process`、`test-vfs` 在比赛镜像 QEMU 中通过；未运行 LoongArch 多任务、时钟抢占或 COW。

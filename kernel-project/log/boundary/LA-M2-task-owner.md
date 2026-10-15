# LoongArch M2 单任务所有权迁移边界

- 日期：2026-10-15
- 分类：RFC；状态：单任务迁移已验证，仍非共享调度器验收

## 目标和非目标

- **目标**：把 LoongArch 首任务的 `MemorySet`、`KernelStack` 与 trap 现场统一交给 `kernel::task` 持有；`write` 的用户指针读取只通过 task owner。
- **非目标**：不接入就绪队列、抢占、fork/exec/wait、FD 表或 COW；本轮仍是单任务 M1.5 行为。
- **Owner**：`kernel::task` 独占首任务资源；`arch::trap` 只保存/恢复寄存器，`kernel::syscall` 只请求当前任务数据。
- **Handoff**：启动代码构造 TCB，发布后切换页表并进入用户态；syscall 通过窄接口读取当前任务空间。
- **Failure**：创建任务失败则 panic 停机；用户指针无效返回 EFAULT；退出打印原有标记并停驻。
- **Cleanup**：当前栈不能在退出时释放，因此退出后任务资源保持到 QEMU 结束。调度器接管时由 task owner 实现回收。

## 保护与验收

- 保护 LoongArch `TrapContext` ABI、`write/exit` 返回语义、RISC-V 现有 task 行为。
- 保持仅 bootstrap 核运行任务；全局 owner 在发布后不再替换。
- 验收：`make ARCH=loongarch64 test-user` 命中 `LA_USER_EXIT_PASS`，`make ARCH=loongarch64 test-la-switch` 命中 `LA_SWITCH_PASS`；RISC-V 编译通过。
- 停止条件：需要改变共享 task 调度、trap ABI 或多核可见行为时停止并另立 RFC。

路线：CKPT 1 搬迁单任务资源所有权并编译；CKPT 2 运行有界 QEMU 回归。后续再将单任务桥并入共享调度器。

agent-run：比赛镜像 `make ARCH=loongarch64 test-user RUN_ID=20261015-la-task-owner-a` 和 `test-la-switch RUN_ID=20261015-la-task-owner-switch` 均为 `MATCHED`；RISC-V `make ARCH=riscv64 check` 通过。未运行 fork/调度/COW。

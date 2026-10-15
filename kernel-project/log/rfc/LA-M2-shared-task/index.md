# RFC：LoongArch 首任务接入共享 task

- 日期：2026-10-15；状态：单任务共享 TCB 路径已运行验证。

## 决策

将 LoongArch 的单任务资源所有权从临时 `loongarch_single.rs` 转交已有共享 TCB。两个架构共用任务创建、登记、当前地址空间访问和退出路径；架构差异仍留在 `arch::task`、`arch::trap` 与启动时的 ELF 选择。

本次只切换 owner，不打开 LA 定时器/外部中断，也不把尚未验收的 fork/wait 等暴露给 LA 用户态。单任务退出后没有可运行任务，停驻而不释放当前内核栈。

## 风险与验收

- 首次 `restore_from` 必须从 TCB 拥有的内核栈读取 trap 现场；LA 汇编在恢复用户态前设置 SAVE0。
- `write` 必须经当前任务的 `MemorySet` 校验用户指针，不能保留第二个地址空间全局变量。
- 有界 QEMU 验证 `LA_USER_EXIT_PASS` 和 `LA_SWITCH_PASS`；RISC-V 编译不回归。

首次运行在编译器内建 `memset` 的 LSX 指令处触发 LoongArch 异常 code 16；
架构层增加基础字节存储实现后通过。此兼容实现位于 `arch/loongarch64/memory.rs`，
不把 QEMU CPU 扩展要求传给公共 task。失败日志保留于隔离测试目录，正式通过证据见下节。

## 验证证据

- agent-run：`make ARCH=loongarch64 test-user RUN_ID=20261015-la-shared-task-b`，`LA_USER_EXIT_PASS`。
- agent-run：`make ARCH=loongarch64 test-la-switch RUN_ID=20261015-la-shared-task-switch`，`LA_SWITCH_PASS`。
- agent-run：RISC-V `test-process RUN_ID=20261015-la-shared-task-rv-process-b` 和 `test-vfs RUN_ID=20261015-la-shared-task-rv-vfs` 均为 `MATCHED`。

## 后续边界

通过本 RFC 只说明共享 task owner 已接入。LoongArch 实际多任务调度、时钟抢占、fork/COW、FD ABI 仍需独立验收。

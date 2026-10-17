# RFC：LoongArch 首任务资源归属

- 日期：2026-10-15
- 状态：单任务 owner 迁移已运行验证；本 RFC 不宣称 M2 调度完成。

## 问题与决策

当前 LoongArch 用户空间由 `kernel/mod.rs` 的全局 `Option<MemorySet>` 持有，内核栈由永不返回的局部变量保持存活。两项资源没有同一个任务生命周期 owner，后续调度接入时容易重复建立真相源。

本次在 `kernel/task` 增加 LoongArch 单任务实现，并经 `kernel::task` 名称暴露窄接口。它持有一个 `TaskControlBlock`，其中统一存放用户空间和内核栈。RISC-V 既有 task 实现不移动、不改 ABI；LoongArch 仍只运行一个用户任务。

## 风险和回退

- 发布 TCB 前不得激活其用户页表；发布后不得释放当前内核栈。
- 用户指针读取持有 task owner 锁，不跨越调度点；本阶段没有异步调度。
- 若 QEMU 中用户态写入或退出回归失败，停止迁移并检查栈顶现场与页表根；不伪称 M2 已完成。

## 退出条件

共享调度器支持 LoongArch、并具有退出回收路径后，删除此单任务实现，改用统一 TCB/就绪队列；本 RFC 的通过不替代该后续验收。

## 验证

- agent-run：`make ARCH=loongarch64 test-user RUN_ID=20261015-la-task-owner-a`，`LA_USER_EXIT_PASS`。
- agent-run：`make ARCH=loongarch64 test-la-switch RUN_ID=20261015-la-task-owner-switch`，`LA_SWITCH_PASS`。
- agent-run：RISC-V `make ARCH=riscv64 check`；LoongArch 多任务、抢占、fork/COW 均未运行。

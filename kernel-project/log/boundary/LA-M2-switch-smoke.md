# LoongArch M2 切换现场烟雾测试边界

- 任务：验证已编译的 LoongArch `TaskContext` 可以在两段独立内核栈之间往返切换。
- 分类：小迭代；日期：2026-10-15；状态：专用 QEMU 测试通过。

## 边界

- **目标**：仅在专用构建 feature 下运行一次内核态 A→B→A 切换，并输出唯一验收标记。
- **非目标**：不接入共享调度器，不开启时钟中断，不宣称 fork/exec/wait 或 COW 已实现。
- **Owner**：`arch::loongarch64::task` 拥有寄存器保存恢复；测试栈由 `KernelStack` 拥有。
- **Handoff**：测试从 `kernel::main` 在 `mm::init` 后进入架构自检，结束后继续原启动链。
- **Failure**：不满足阶段断言即 panic；不吞掉错误继续进入用户态。
- **Cleanup**：恢复 A 栈后释放 B 栈；全局测试指针清零，正式构建不包含测试调用。

## 受保护契约与验证

- 默认 LoongArch 和 RISC-V 构建行为不变；`TaskContext` 字段布局与汇编偏移不变。
- `make ARCH=loongarch64 test-la-switch` 在有界 QEMU 内看到 `LA_SWITCH_PASS`。
- 同时检查双架构编译及格式；尚未运行共享 task 调度时必须如实标注。
- 停止条件：需要修改 trap ABI、公共 task owner 或中断协议；这些另行走 RFC。

路线：CKPT 1 加入仅测试 feature 与自检入口并编译；CKPT 2 运行有界 QEMU 验证往返切换。

## 验收记录

- agent-run：比赛镜像 `make ARCH=loongarch64 test-la-switch RUN_ID=20261015-la-switch-b`，`MATCHED`，串口含 `LA_SWITCH_PASS`。
- agent-run：同镜像 `make ARCH=loongarch64 test-user RUN_ID=20261015-la-switch-regress`，`MATCHED`，原用户态出口仍输出 `LA_USER_EXIT_PASS`。
- agent-run：RISC-V `make ARCH=riscv64 check` 与 `make fmt-check` 通过；未运行共享 task 调度或 fork/COW。

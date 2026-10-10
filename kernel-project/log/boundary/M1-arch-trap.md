# Implementation Boundary（M1 架构 trap 骨架）

## 基本信息

- 任务：建立 RISC-V64 与 LoongArch64 的架构 trap 入口、上下文和公共 handoff
- 分类：小迭代
- 日期：2026-10-10

## 边界

- **目标**：两个架构均能安装 trap 入口，在当前内核栈保存完整整数现场和关键 CSR，向 `kernel::trap` 交付统一可查询的 `TrapContext`，并具备返回汇编路径。
- **非目标**：不进入用户态，不切换用户/内核栈，不启用中断，不处理 syscall、缺页、时钟、外设中断或调度。
- **Owner**：`arch/*/trap.rs` 拥有寄存器布局和 CSR 操作；`kernel/trap` 拥有架构无关的 trap 语义与处理策略。
- **Handoff**：架构汇编以 `&mut arch::TrapContext` 调用 C ABI 符号 `kernel_trap_handler`。
- **Failure**：当前公共 handler 对任何 trap 打印诊断后停机，禁止吞掉未知内核异常。
- **Cleanup**：trap frame 位于当前栈，正常返回路径恢复现场并回收；当前 fatal handler 不返回。

## 受保护契约（不得破坏）

- public API：现有 `crate::arch::{console, park, restart, power_off}` 保持不变。
- ABI / visible semantics：正常 M0 启动日志和停机行为保持不变；不自动制造 trap。
- current contract：只有 bootstrap core 使用单一 boot 栈，未建立用户态与多核 trap 栈。
- correctness invariant：汇编偏移必须与 `#[repr(C)] TrapContext` 一致；进入 Rust 前保存所有可见整数寄存器；返回前恢复 PC、状态和寄存器。

## 验收与验证

- **验收标准**：`make all` 生成双架构 ELF；ELF 中存在各自 trap 入口和 `kernel_trap_handler`；反汇编确认入口写入对应 CSR 且返回指令分别为 `sret` / `ertn`。
- **验证方式**：比赛容器内 `make all`、`readelf`、`objdump` 静态检查。
- **验证者**：agent-run。
- **停止条件**：若需要用户栈切换、Task 所有权、中断控制器确认或运行时 QEMU probe，则停止并另开阶段。

## 路线（小迭代）

- CKPT 1：双架构 arch trap 骨架 + 公共 fatal handoff → 验证：双架构链接与反汇编。

## 升级触发

用户态切栈、调度中返回到另一任务、嵌套中断或修改 syscall ABI 时升级并重新声明边界。

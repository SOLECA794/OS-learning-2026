# 2026-10-10 M1 trap 分发

## 变更

- `kernel/trap.rs` 按统一 `TrapCause` 分类处理 trap。
- 用户态系统调用从架构上下文提取调用号和六个参数，推进返回 PC，调用 `kernel::syscall`，并写回返回值。
- 未支持的页错误、中断、断点、非法指令和未知异常保留明确的致命停机策略。
- 更新 LoongArch trap 入口说明，使其与已经激活的分页状态一致。

## 验证

- `agent-run`：在 `zhouzhouyi/os-contest:20260510` 中执行 `make fmt && make all`，
  RISC-V64 与 LoongArch64 均编译、链接和镜像检查通过。
- RISC-V64 ELF 入口保持 `0x80200000`；LoongArch64 ELF 入口保持 `0x200000`。
- `not-run`：未启动 QEMU；任务级内核栈和用户态入口尚未实现，不能安全触发用户系统调用。

## 架构摩擦扫描

- 公共 trap 层只使用统一上下文方法，不读取 RISC-V CSR 或 LoongArch CSR。
- 系统调用 ABI 的寄存器差异留在各自 `TrapContext` 实现中。
- 中断原因暂时保持原始值，只用于故障日志；真正的中断控制器抽象留到中断阶段。

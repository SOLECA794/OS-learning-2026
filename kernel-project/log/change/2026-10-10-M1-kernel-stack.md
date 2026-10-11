# 2026-10-10 M1 内核栈内存

## 变更

- 页帧分配器增加从未使用 bump 区获取连续页的窄接口。
- `FrameBlock` 作为连续页范围的唯一所有权令牌，析构时逐页归还。
- `KernelStack` 固定持有 4 页（16 KiB），提供架构无关的栈底、栈顶和大小查询。
- 暂不加入 TaskContext 和上下文切换；本阶段仅完成内存所有权。

## 验证

- `agent-run`：在 `zhouzhouyi/os-contest:20260510` 中执行 `make fmt && make all`，
  RISC-V64 与 LoongArch64 均编译、链接和镜像检查通过。
- RISC-V64 ELF 入口保持 `0x80200000`；LoongArch64 ELF 入口保持 `0x200000`。
- `not-run`：不启动 QEMU，也不实际切换到新栈。

## 架构摩擦扫描

- 栈实现只通过 `arch::mm::phys_to_virt` 获得内核可访问地址，没有读取架构寄存器。
- 双架构共享相同的页帧所有权和 ABI 对齐约束。
- 当前连续物理页方案依赖身份映射；切换高半地址时只需替换本模块的映射策略。

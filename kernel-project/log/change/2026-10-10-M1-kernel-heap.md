# 2026-10-10 M1 内核堆

## 变更

- 按 rCore 风格命名为 `heap_allocator`，新增 1 MiB 固定 backing 的内核全局分配器。
- 空闲区按地址排序，分配时处理布局对齐和过小碎片，释放时合并相邻区域。
- 每次 allocation 使用内部 header 记录真实占用范围，避免对齐填充和尾部吸收造成泄漏。
- 增加 allocation error handler，以及 `Box` / `Vec` 启动期分配释放自检。
- 内核内存初始化完成后启用堆并输出总量、空闲量和存活 allocation 数。
- 修复 `heap_allocator.rs` 文件头声明损坏，并为链表区间、allocation 区间与碎片地址计算补充整数溢出检查。

## 验证

- `agent-run`：在 `zhouzhouyi/os-contest:20260510` 中执行 `make fmt && make all`，
  RISC-V64 与 LoongArch64 均编译、链接和镜像检查通过。
- `not-run`：未启动 QEMU，因此启动期 heap self-test 和 `mm: kernel heap active` 日志尚未获得运行证据。

## 架构摩擦扫描

- 堆算法完全位于公共 `kernel/mm`，仅通过已有 `phys_to_virt` handoff 访问 backing。
- 全局分配 API 和对象布局不包含架构条件编译。
- 固定连续 backing 是 M1 启动策略，后续按页扩容只替换 backing 管理，不影响 `alloc` 使用方。

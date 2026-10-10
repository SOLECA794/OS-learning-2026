# 2026-10-10 M1 MemorySet

## 目标

实现双架构共享的 `MemorySet` 基础，但保持 MMU 关闭。

## 设计

- 使用 4 KiB、三级、每级 512 项的公共页表遍历模型。
- 架构后端负责把公共 `MapPermission` 编码为本架构 PTE。
- 物理页分配器使用连续页区间和逐页状态表；当前单一 RAM 区间来自 QEMU 配置。
- 在尚未引入内核堆前，`MemorySet` 使用固定上限的区域槽；达到上限返回错误，不静默覆盖。

## 验证

- `agent-run`：在 `zhouzhouyi/os-contest:20260510` 临时副本中执行 `make all`，RISC-V64 与 LoongArch64 均编译、链接和镜像检查通过。
- RISC-V64 入口保持 `0x80200000`；LoongArch64 入口保持 `0x200000`。
- `not-run`：未启动 QEMU，因此页表构造的运行日志尚未观测。

## 架构摩擦扫描

- 页帧状态只有全局 `FrameAllocator` 一份；`FrameTracker`、framed `MapArea` 与 `PageTable` 分别持有数据页、叶子页和页表页的唯一所有权，不复制 allocation 状态。
- 架构私有表示没有泄漏到 `kernel/mm`；公共层只调用统一的 `PageTableEntry` 接口。
- 两个临时桥都有失败行为与退出条件：QEMU RAM 配置不受支持时 panic；64 个区域槽耗尽时返回 `AreaLimit`。平台内存发现和 kalloc 落地后分别移除。

# G1 边界：连续页复用（开工前）

依据 V2 §5：连续区（内核栈、用户地址空间）在反复分配/释放后可复用。

## 目标

1. `allocate_contiguous(N)` 释放后能再次满足连续 N 页请求（不依赖 bump 无限前进）。
2. 8 轮 fork/exit/wait：帧数每轮对称（无泄漏）+ bump 水位不随轮次增长（复用生效）。
3. 回归：smp4、fault-test、LA build 不退化。

## 非目标

- 不做 buddy/伙伴系统（G6+ 排期）。
- 不改 KernelStack 为物理离散（虚拟栈映射排期 M 系列）。
- 不动分配接口签名（allocate/allocate_contiguous/deallocate 语义不变）。

## 验收

- 主链路 8 轮 PASS，frames 1815/1292 逐轮对称。
- wm（bump 水位）第 1 轮后恒定（第 2 轮起从空闲块切）。
- 稳定性 ×2、smp4、fault-test、LA build 全 PASS。

## 风险与回滚

- 空闲块链表元数据写坏 → 分配器断言失败（deallocate_range 有范围/计数断言兜底）。
- 回滚：git revert 到 W4 27b75fe 前 frame_allocator.rs 版本。

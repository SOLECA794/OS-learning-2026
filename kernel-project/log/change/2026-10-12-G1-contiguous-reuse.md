# G1 回填：连续页复用修复 + 反复 fork/exit 验收

日期：2026-10-12
依据：V2 总路线 §5「先修正真实回收问题」——连续页分配仅推进 bump 游标，FrameBlock
释放回单页链表，连续块无法复用。属 G1 门槛回填，前置 W4（27b75fe）。

## 实现

frame_allocator.rs 重构为**空闲块链表**：

- 空闲块首页保存元数据 `[size][next]`（块内页数 + 下一块首页页号，`usize::MAX`=尾）；
  块按起始页号升序排列。
- `allocate_from_blocks`：从块尾切出所需页数（保持块头与链表指针不变）；整块消费时摘除；
  剩余部分改写 size。
- `allocate` / `allocate_contiguous`：优先从空闲块切，失败才前进 bump 游标。
- `deallocate_range`：按地址排序插入 + 与相邻块合并（可连续吸收多个后继块）；
  并入前驱或作为新块插入。
- `FrameBlock::drop` 整段一次归还（不再逐页）；`FrameTracker::drop` 走单页路径。
- 新增诊断接口 `bump_watermark()`（bump 游标前进量），fork/wait 日志带 `wm=` 观测。

## 根因

旧实现 `recycled` 是单页链表；`allocate_contiguous` 只从 bump 分配。内核栈（4 页连续）
与用户地址空间（~519 页）每轮 fork 都消耗新 bump 页，释放后无法再次凑成连续块——
长时间运行后 bump 耗尽，但空闲单页无法用于栈。V2 §5 点名，证据要求
「重复创建/退出后的页数与分配可用性」。

## 验证矩阵（agent-run，WSL Ubuntu-24.04 + QEMU 9.2.1）

| 项 | 结果 |
|---|---|
| RV make build | PASS，e_entry=0x80200000 |
| 主链路 10s（8 轮 fork/exit/wait） | PASS：8 轮 `[fork] frames=1815 → [wait] frames=1292` 完全对称，无泄漏 |
| **bump 水位 wm**（连续页复用决定性证据） | **wm 恒 1815**：第 1 轮 fork 后水位不再增长，第 2-8 轮全部从空闲块复用（旧实现每轮 +523，第 8 轮达 ~4953） |
| 稳定性 ×2 | PASS（pass=1 fail=0 fatal=0） |
| -smp 4 | PASS（从核 park 不干扰） |
| fault-test 回归 | PASS |
| LA make build | PASS（e_entry=0x200000） |

## 设计说明

- 块尾切分：切出的段不含元数据（元数据在块头），返回段整页可用；分配前清零覆盖元数据。
- 合并顺序：先吸收后继（可多个），再并入前驱——保证链表升序与不重叠不变量。
- 空闲块元数据写入/读取均为 unsafe，理由标注：空闲页由分配器独占、分配前清零覆盖。
- wm 观测点：`frames`（allocated）证明无泄漏，`wm`（next-start）证明不复用才增长。

## 残留风险

- 空闲块链表为线性扫描：最坏 O(空闲块数) 分配/释放。当前规模（单核、数百块）可接受；
  SMP/大规模内存阶段改 buddy/伙伴系统（V2 G6+ 排期）。
- 元数据占用首页 16 字节：任何分配出的页在清零前不得被读取（当前所有分配路径都先清零）。

## Architecture Friction Scan

Safe：无阻断摩擦。Euclid（记录不修）：线性空闲块链表 → buddy；wait4 轮询重查（W4 已登记）。

# 双架构页帧回收自检边界

- 分类：小迭代；日期：2026-10-14。
- 目标：启动时验证单页与连续页释放/相邻合并/再次分配，且分配计数归零；拒绝回收与现有空闲块重叠的区间。
- 非目标：物理内存探测、跨 NUMA 分配、高内存、并发压力测试、LoongArch 用户态。
- Owner：`frame_allocator` 独占空闲块链表与页数统计；`mm::init` 只调用自检。
- Handoff：分页激活、堆自检完成后执行；自检令牌析构归还页帧，不把地址暴露给其他子系统。
- Failure：断言失败停在启动阶段，不能打印 M1 成功标记；不会继续启动有损坏的分配器状态。
- Cleanup：所有 `FrameTracker` / `FrameBlock` 由 RAII 释放，结束时 `allocated_count` 与调用前一致。
- 受保护：页帧 API、内核页表/堆映射、RISC-V 用户态行为、LoongArch M1 启动行为、现有 `make test` 标记。
- 验收：比赛镜像中 RISC-V 和 LoongArch 各自 `make test` 到自检标记及 M1 标记，RISC-V VFS/进程回归；双架构构建、格式检查。
- 停止：如果 LoongArch 需修改架构页表或现有启动顺序才能通过，停止并单独上报。

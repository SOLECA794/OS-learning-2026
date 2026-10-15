# CHANGELOG

> 本文件汇总 `kernel-project/log/change/` 下的每次改动，按时间倒序。
> 维护规则：每个 commit 合并进 master 后，在本文件顶部新增一条；
> 详细内容仍以 `log/change/<日期>-<描述>.md` 为准。

## [Unreleased]

- style: 清理 11 个 dead_code warnings + COW 引用计数 debug 防护（commit 0095e30）
  - address.rs / memory_set.rs / task / arch trap / kernel_stack / syscall 标 allow(dead_code) 并注释保留原因
  - 删除重复定义的 EBADF
  - frame_allocator: cow_retain/deallocate_cow 加 debug_assert 计数边界 + 锁顺序注释
  - 测试矩阵全绿：build(0 warnings) / test / test-fault / run-cow(128轮) / test-shell / test-vfs / test-cow-oom

## [2026-10-15]

- 合并队友 yyw 分支 e009b6d（VFS 重构 + M3 ABI + 双架构内存自检 + COW OOM/reclaim 检查工具）
- fast-forward 合并 ccdf46b..e009b6d -> master

## [2026-10-14]

- M28: COW OOM 520 点故障注入测试
- M28: wait + COW 回收 128 轮
- M3: frame 复用双架构对齐、memory_set 双架构、mkdirat dirfd
- G6 补丁: 内核态 COW 写、用户态 read copy

## [2026-10-13]

- G6: COW fork 完整落地（软件 COW 集合 + PTE 清写位）

## [2026-10-12]

- G0: QEMU 有界运行器 + KTEST 事件
- G1: 连续页复用
- G4: exec
- G5: ramfs + FD + shell
- W1: P0/P1 修复
- W2: syscall errno + fault 测试（EFAULT）
- W3: 上下文切换 + 用户态时钟轮转
- W4: fork/exec/exit/wait 进程生命周期

## [2026-10-10 ~ 10-11]

- M0 scaffold、M1 架构 trap / 分页 / 内核栈堆 / 地址空间
- QEMU boot 修复（921 对齐等）

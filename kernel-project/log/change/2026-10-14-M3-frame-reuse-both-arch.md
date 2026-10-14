# 双架构页帧回收与复用自检

- 日期：2026-10-14
- 分类：小迭代；边界见 [M3-frame-reuse-both-arch](../boundary/M3-frame-reuse-both-arch.md)
- 验证者：agent-run；环境：比赛镜像 `zhouzhouyi/os-contest:20260510`、QEMU 10.0.2、4 核、1024 MiB。

## 改动

`frame_allocator::self_test()` 在分页和堆初始化后、M1 成功标记前执行。自检从一段独占连续页派生两个相邻所有权令牌，逆序归还后再次分配完整区间，核对复用及 bump 水位不增长；又向单页首尾写入测试值，归还并重新分配后核对整页清零；最终已分配页数必须恢复。`FrameBlock::split_at` 仅在分配器模块内部使用，不扩大外部 API。

回收路径在空闲块链表扫描时断言不存在与待归还范围重叠的块，防止错误的重复/重叠释放静默破坏链表。正常路径的 owner 与 RAII 释放协议不变。

## 运行证据

| 命令 | 结果 |
|---|---|
| 比赛镜像中 `make ARCH=riscv64 test RUN_ID=20261014-frame-reuse-rv` | PASS；`artifacts/runs/20261014-frame-reuse-rv/`；`M3-FRAME-REUSE-PASS` 在 M1 标记之前 |
| 比赛镜像最终源码隔离副本中 `make clean`、`make ARCH=loongarch64 defconfig`、`make ARCH=loongarch64 test RUN_ID=20261014-frame-reuse-la-final` | PASS；结果归档 `artifacts/runs/20261014-frame-reuse-la-final/`；`M3-FRAME-REUSE-PASS` 与 M1 标记；仅验证启动期内存层 |
| `make ARCH=riscv64 test-vfs RUN_ID=20261014-frame-reuse-final` | PASS；`artifacts/runs/vfs-20261014-frame-reuse-final/` 含 `VFS_PASS` |
| `make ARCH=riscv64 test-cow-oom RUN_ID=20261014-frame-reuse-final` | PASS；`artifacts/runs/oom-20261014-frame-reuse-final/`；128 轮回收后 1301 帧，520 个地址空间故障点、9 个 fork 早期点通过 |
| `make fmt-check`、`git diff --check` | PASS |

## 边界和摩擦扫描

页帧状态仍只有全局 `FrameAllocator` 一份真相；测试令牌只在自检栈上存在，RAII 归还后不再访问。相邻块来源于同一初始 `FrameBlock`，不会依赖启动时空闲链表布局。重叠断言是 fail-stop 校验，不替代调用者唯一所有权的安全契约。LoongArch `make test` 的匹配点是 M1，当前 `restore_from` 仍停驻，不能声称其用户态、系统调用、进程或 COW 已运行验证。

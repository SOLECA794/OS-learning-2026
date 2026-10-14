# 双架构 MemorySet 映射与回收自检

- 日期：2026-10-14
- 分类：小迭代；边界见 [M3-memoryset-both-arch](../boundary/M3-memoryset-both-arch.md)
- 验证者：agent-run；比赛镜像 `zhouzhouyi/os-contest:20260510`、QEMU 10.0.2、4 核、1024 MiB。

## 改动与约束

`mm::init` 在 heap 初始化后调用 `memory_set::self_test`。自检只创建未激活的临时页表：插入两页 `Framed` 用户映射，检查两个 VPN 对应不同页帧、架构 PTE 读回 `READ|WRITE|USER` 和普通内存类型；验证重叠插入被拒；删除区域后确认映射消失、二次删除报 `AreaNotFound`，地址空间析构后物理页数量恢复。成功才输出 `M3-MEMORYSET-PASS`。

## 运行证据

| 命令 | 结果 |
|---|---|
| `make ARCH=riscv64 test RUN_ID=20261014-memoryset-rv` | PASS；`artifacts/runs/20261014-memoryset-rv/` 含 `M3-MEMORYSET-PASS`、M1 标记 |
| 比赛镜像隔离副本中 `make clean`、`make ARCH=loongarch64 defconfig`、`make ARCH=loongarch64 test RUN_ID=20261014-memoryset-la` | PASS；证据复制到 `artifacts/runs/20261014-memoryset-la/`，含 `M3-MEMORYSET-PASS`、M1 标记 |
| `make ARCH=riscv64 test-vfs RUN_ID=20261014-memoryset` | PASS；`artifacts/runs/vfs-20261014-memoryset/` 含 `VFS_PASS` |
| `make ARCH=riscv64 test-cow-oom RUN_ID=20261014-memoryset` | PASS；`artifacts/runs/oom-20261014-memoryset/`；128 轮回收后 1301 帧，520 个地址空间故障点、9 个 fork 早期点通过 |
| `make fmt-check`、`git diff --check` | PASS |

## 摩擦扫描与剩余边界

映射/数据页仍由临时 `MemorySet` 唯一持有，中间页表仍由 `PageTable` 析构；测试不激活临时页表、不改活动内核根页表、不向任务发布第二份状态。两个架构的 PTE 软件编解码和地址空间资源回收获运行验证；LoongArch 用户态切换、TLB 硬件访问、进程/COW/文件系统调用未运行，不能由本测试推断通过。

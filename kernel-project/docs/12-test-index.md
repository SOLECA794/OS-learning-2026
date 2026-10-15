# 12 · 测试索引

> 6 项有界回归入口对照表。外部读者按本文件复现验证。

| 入口 | Make 目标 | 匹配 pattern | 后处理脚本 | 验证内容 |
|---|---|---|---|---|
| 基础回归 | `make test` | — | — | 启动 + 基础 syscall 冒烟 |
| 缺页注入 | `make test-fault` | — | — | m28-frame-fault-test feature 故障点 |
| COW 回收 | `make run-cow` | `G6-COW-PASS` | `tools/check_cow_reclaim.py` | 128 轮 fork/exit 后 frames 数、forks/copies 计数 |
| shell | `make test-shell` | `shell: done` | — | 小 shell 脚本执行完成 |
| VFS | `make test-vfs` | `VFS_PASS` | — | VFS 层 openat/read/write/close/dup |
| COW OOM | `make test-cow-oom` | — | `tools/check_cow_oom.py` | 520 点地址空间 + 9 个早期 fork 点 + 跨页写入 OOM |

## 运行环境

- WSL Ubuntu-24.04，qemu-system-riscv64
- 构建：`python3 tools/kconfig.py --defconfig config/defconfig-riscv64`（clean 后必跑）
- 产物：`build/riscv64/kernel.elf`，每次 run-id 存 `artifacts/runs/<run-id>/`

## 通过标准

- 构建：0 warnings
- 6 项测试：全部 PASS（run_qemu.py 退出码 0 + pattern MATCHED）
- 后处理脚本：输出 PASS 行

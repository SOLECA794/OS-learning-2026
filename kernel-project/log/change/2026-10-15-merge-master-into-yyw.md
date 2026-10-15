# 将 master 新进度合入 yyw

- 日期：2026-10-15；验证者：agent-run；方向：`origin/master` → `yyw`。
- 来源：`origin/master` 1387858；合入前 `yyw` 2f385af。

## 冲突处理

- `master` 的 brk、mmap/munmap、pipe/dup2、VirtIO-Blk 和文档全部保留。
- 文件系统沿用 `yyw` 已验收的 `kernel/fs` VFS owner；新 Pipe 对象放在 `kernel/fs/pipe.rs`，FD 表只持有 `Arc<Pipe>`。
- RISC-V 新增三种用户态测试镜像在 `arch/riscv64/task.rs` 选择；LoongArch 继续由其架构入口选择本架构 ELF。共享 task 不保留两份镜像选择器。
- VirtIO-Blk 固定 MMIO 探测仅在 RISC-V QEMU virt 启动路径调用，LoongArch 平台地址未验证前不触碰该寄存器。

## 运行验收

所有测试均在比赛镜像 `zhouzhouyi/os-contest:20260510` 运行，通过项目 `make` 入口。

| 架构 | 命令与 RUN_ID | 结果 |
|---|---|---|
| RISC-V | `test-process RUN_ID=20261015-merge-process` | PASS，最终退出码 7 |
| RISC-V | `test-brk RUN_ID=20261015-merge-brk` | PASS，`BRK-PASS` |
| RISC-V | `test-pipe RUN_ID=20261015-merge-pipe` | PASS，`PIPE-PASS` |
| RISC-V | `test-mmap RUN_ID=20261015-merge-mmap` | PASS，`MMAP-PASS` |
| RISC-V | `test-vfs RUN_ID=20261015-merge-vfs` | PASS，`VFS_PASS` |
| RISC-V | `test RUN_ID=20261015-merge-basic` | PASS，基础启动 |
| RISC-V | `test-fault RUN_ID=20261015-merge-fault` | PASS，用户缺页受控停机 |
| RISC-V | `run-cow RUN_ID=20261015-merge-cow` | PASS，128 轮帧回收 |
| RISC-V | `test-shell RUN_ID=20261015-merge-shell` | PASS，`shell: done` |
| RISC-V | `test-cow-oom RUN_ID=20261015-merge-cow-oom` | PASS，520+9 故障点 |
| LoongArch | `test-la-exec RUN_ID=20261015-merge-la-exec` | PASS，`LA_EXEC_PASS`、退出码 0 |
| LoongArch | `test-la-process RUN_ID=20261015-merge-la-process` | PASS，fork/COW/exit/wait4 |

`make fmt-check` 在接收 master 原文件后先发现未格式化处；`make fmt` 后复查通过。`git diff --check` 在合并收口时复查。

## 剩余边界

Pipe 当前是非阻塞的最小环形缓冲；VirtIO-Blk 只完成固定 QEMU 地址的探测/轮询框架，不能宣称通用块设备可用。brk/mmap/pipe 的用户 ABI 仅在 RISC-V 验收；LoongArch 尚未对齐。未引入第二份 VFS owner 或共享任务镜像选择器。

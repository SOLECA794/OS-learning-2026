# M3 `wait4` 32 位状态字段

- 状态：Closed（RISC-V 单次状态宽度切换与回归已完成）
- 日期：2026-10-14
- 作者：Codex；评审：用户在 M2.8 收口并获知 M3 ABI 兼容为下一阶段后要求“继续推进”，覆盖此窄范围。

## 目标与现状

Linux `wait4` 的 `wstatus` 是 `int *`。当前 `sys_wait4` 通过 `usize::to_ne_bytes()` 写 8 字节，覆盖紧邻状态的 4 字节；仓库用户程序也以 `usize` 接收。M3 首个兼容切换将内核写回与仓库调用方统一为 32 位，同时保留 M2.8 的“写回成功才回收”事务顺序。

## 设计与边界

- 语义边界详见 `../../boundary/M3-wait4-status-width.md`。
- ABI Replace：仅状态对象宽度从 8 字节改为 4 字节，编码保持 `((exit_code & 0xff) << 8)`。按原始字节复制，避免对未对齐用户指针产生 Rust 引用。
- 无过渡桥：旧宽度会写坏相邻内存，继续支持会破坏新契约。未迁移的仓库外程序需要使用 32 位 `int`。
- 非目标：PID 组选择、完整 flags、rusage 写回、LoongArch 用户态，另行规划。

## 一次切换与验收

先把测试改为 32 位状态与 32 位 canary，确认旧内核无法通过；随后修改内核写回并同步所有仓库调用方。只有 QEMU 定向及相关回归通过后，README 才更新当前 ABI 描述。失败时保持原状的源码差异供审查，不做工作区回滚。

## 收口记录

- 修复前：`make ARCH=riscv64 run-cow RUN_ID=20261014-wait32-before` 的串口输出 `M3-WAIT32-FAIL`、`c8-FAIL`，有界运行器 60 秒后以 `EXIT_TIMEOUT` 退出。旧 8 字节写回破坏紧邻 canary。
- 修复后：`make ARCH=riscv64 run-cow RUN_ID=20261014-wait32-after` PASS；`M3-WAIT32-PASS`、`WAIT-RETRY-PASS`、`G6-COW-PASS`；128 轮回收帧数稳定。
- 回归：`make ARCH=riscv64 test-process/test-vfs/test-shell/test-console RUN_ID=20261014-wait32` 分别 PASS；`make ARCH=riscv64 test-cow-oom RUN_ID=20261014-wait32` PASS（520 个 COW 子地址空间故障点、完整 fork 9 个早期故障点）。产物与串口证据在 `artifacts/runs/` 的同名运行目录下。
- 构建：`make fmt-check`、`git diff --check`、普通 RISC-V `make build` PASS；LoongArch 在容器临时副本中 `make clean`、`make ARCH=loongarch64 defconfig`、`make ARCH=loongarch64 build` PASS，入口 `0x200000`。LoongArch 用户态没有本轮运行结论。
- 契约：系统调用号、退出码编码、EFAULT 保留子任务、一次回收、task owner 均保持。唯一 ABI 替换是状态对象宽度 8→4 字节，已迁移所有仓库内调用方。
- 摩擦扫描：`copy_wait_status_to_user` 只在 syscall 边界编码并走已有 `copy_bytes_to_user`，没有第二份状态 owner；canary 仅在测试程序，生产路径无特判。遗留的 PID 组/options/rusage 语义和 `openat` 私有 ABI 路由到后续 M3 工作。

# Change Record · 本地 QEMU 对齐官方评测版本（9.2.1）

- 日期：2026-10-11
- 分类：环境治理（非内核代码变更）
- 影响范围：本地 WSL 开发环境（仅 WSL 侧，Windows 侧工具链不动）

## 背景

M0 change record（2026-10-11-M0-qemu-boot-fixes.md）环境注记指出：官方评测环境为
`zhouzhouyi/os-contest:20260510`（内置 QEMU 9.2.1），本地 WSL 为 apt 版 8.2.2。
为消除"本地通过、评测环境行为差异"的风险，将本地 WSL QEMU 对齐至 9.2.1。

## 过程与决策

1. 尝试 `docker pull zhouzhouyi/os-contest:20260510`（直连 Docker Hub）→ 拉取中断
   （`short read: unexpected EOF`）；改用国内加速源 `docker.1ms.run` 重试 → 同样中断。
   结论：当前网络环境下 Docker Hub 拉取不可靠，不阻塞在该路径。
2. 改走源码编译：下载 QEMU 9.2.1 源码（GitHub tag，39.8 MB），
   `--target-list=riscv64-softmmu,loongarch64-softmmu --disable-werror --disable-docs`，
   gcc 13.3 / ninja 1.11 / glib 2.80，22 核并行约 2 分钟完成。
   - 安装前缀：`/home/kkio/qemu-9.2.1`（非 root 用户，未写 /opt）
   - 注意：QEMU 9.2.1 的 configure **无 `--disable-tests` 选项**（8.x 有），首次尝试因此失败一次。

## 环境变更

- `~/.bashrc` 追加 `export PATH=/home/kkio/qemu-9.2.1/bin:$PATH`（新 shell 生效）。
- WSL 内 `qemu-system-riscv64` / `qemu-system-loongarch64` 解析到 9.2.1。
- Windows 侧：无改动（Windows 仍无 QEMU，构建产物用 WSL 侧 9.2.1 运行）。

## 验证

| 检查项 | 结果 | 验证者 |
|---|---|---|
| `qemu-system-riscv64 --version` | `QEMU emulator version 9.2.1` | agent-run |
| `qemu-system-loongarch64 --version` | `QEMU emulator version 9.2.1` | agent-run |
| RV M0 真实启动（9.2.1, -smp 4, -m 1G） | `M0 boot OK`，**hartid=3**（OpenSBI 又换了启动核） | agent-run |
| LA M0 真实启动（9.2.1, -smp 4, -m 1G） | `M0 boot OK`，hartid=0 | agent-run |
| 与 8.2.2 行为对比 | 语义一致；boot hart 选择随 QEMU/OpenSBI 版本变化（8.2.2 实测 2，9.2.1 实测 RV=3/LA=0），原子选主不受影响 | agent-run |

## 残余风险 / 缺口

1. **WSL 无 Rust 工具链**（`make: cargo: No such file or directory`）：`make run`
   全链路在 WSL 侧不可用（build 阶段失败）。当前等价路径：Windows 侧构建产物 +
   WSL 侧 9.2.1 直接运行（本次验证即此方式）。如需 WSL 内 `make run` 一步到位，
   需在 WSL 安装 rustup + riscv64gc/loongarch64 target（未执行，超出本次范围）。
2. 源码编译的 9.2.1 与评测镜像内置 9.2.1 构建配置可能有差异（本机构建仅含
   riscv64/loongarch64 两个 target，未启用测试/文档）；核心模拟行为一致。
3. 官方 Docker 镜像仍推荐在 M1 起作为最终验收环境（工具链整体对齐，不只是 QEMU）。

# <PROJECT_NAME>（暂定名，定名后全局替换）

从 0 自研的 **Linux 兼容多核宏内核**，使用 Rust 编写，面向操作系统大赛内核实现赛道。

> 项目名使用 `<PROJECT_NAME>` 占位，团队定名后全局替换（README、docs、Cargo.toml、Makefile）。

## 一句话定位

一个系统调用兼容 Linux UAPI、支持 RISC-V64 与 LoongArch64 双架构、在 QEMU 与实板上可运行的宏内核，功能正确性与性能并重，全部代码自研。

## 核心决策（已定，详见 docs/）

| 决策 | 结论 | 依据文档 |
|---|---|---|
| 内核形态 | Linux 兼容宏内核 | docs/01-architecture.md |
| 语言 | Rust（no_std） | docs/01-architecture.md |
| 架构 | RISC-V64 主线 + LoongArch64 同源 | docs/01-architecture.md |
| 兼容目标 | 对齐 Linux UAPI（6.x 语义），不复制 Linux 内部对象模型 | docs/05-syscall-abi.md |
| 构建 | Kconfig → build.mk → Cargo feature | docs/07-build-and-ci.md |
| 开发治理 | SDD 治理（引用 `../sdd-governance/`） | docs/08-development-process.md |

## 文档导航

- 定位与目标：docs/00-vision.md
- 总体架构：docs/01-architecture.md
- 子系统设计：docs/02-memory.md / 03-task-scheduling.md / 04-filesystem.md / 05-syscall-abi.md / 06-networking.md
- 构建与 CI：docs/07-build-and-ci.md
- 开发流程：docs/08-development-process.md
- 编码规范：docs/09-coding-standards.md
- 测试与验证：docs/10-testing-and-validation.md
- 开发治理（AGENTS.md / skills / 模板）：`../sdd-governance/`

## 快速开始

前置：Rust 工具链由 `rust-toolchain.toml` 锚定（含 `riscv64gc-unknown-none-elf` 与
`loongarch64-unknown-none` 两个 target），`rustup` 会自动准备；运行需要 QEMU（>= 8.0）。

```bash
# 1) 选架构生成配置（架构是 Kconfig 的 choice，切换要连 defconfig 一起重来）
python tools/kconfig.py --defconfig config/defconfig-riscv64     # 或 defconfig-loongarch64

# 2) 编译检查 / 构建
make check
make build            # 产物 build/<ARCH>/kernel.elf，并校验 ELF 入口地址

# 3) 换架构（完整动作，不能只切 ARCH）
make ARCH=loongarch64 defconfig clean build

# 4) QEMU 运行
make run

# 5) 调整日志级别等选项
python tools/kconfig.py --set LOG_LEVEL_TRACE=y
```

其它目标：`make gdb`（等 GDB 连接）、`make fmt` / `make fmt-check`、`make clean`。
`make test` 在 M0 尚未接入测例（docs/10：用户态测例随 M1 的 init 进程落地）。

## 仓库结构（M0）

```
kernel-project/
├── AGENTS.md          # AI 协作治理（源自 ../sdd-governance/AGENTS.md，仓库地图为本仓库实际内容）
├── Makefile / build.mk    # 构建入口与 Kconfig→Cargo feature 转译
├── config/            # Kconfig、defconfig-riscv64 / defconfig-loongarch64
├── linker/            # riscv64.ld（0x80200000）、loongarch64.ld（0x00200000）
├── tools/             # kconfig.py（自研解析器）、check-image.py（映像门禁）
├── src/arch/          # 唯一架构差异入口：riscv64 / loongarch64
├── src/kernel/        # 核心子系统（M0 只有 kernel::main）
├── src/klib/          # 日志宏等最底层通用库
└── log/               # boundary / change-record 等证据（见 ../sdd-governance/）
```

## 状态

- 阶段：M0（地基：规范、工具链、双架构 boot）
- 已落地：SDD 治理落地、Kconfig→build.mk→Cargo feature 构建链、双架构 boot 汇编（已反汇编核对）、可复现构建（SHA-256 校验通过）
- 未完成：QEMU 上的实际启动与串口输出验证（`make run`，本机无 QEMU），M0 验收尚未关闭
- 里程碑：见 docs/08-development-process.md（不含时间排期）

## 许可

见 LICENSE.md。默认建议 MIT OR Apache-2.0。

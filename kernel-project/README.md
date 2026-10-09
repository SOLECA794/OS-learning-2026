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

## 快速开始（占位，待 M0 落地后填充真实命令）

```bash
make menuconfig      # 配置架构与 feature
make build           # 构建
make run             # QEMU 运行
make test            # 测试套件
```

## 状态

- 阶段：M0（地基：规范、工具链、双架构 boot）
- 里程碑：见 docs/08-development-process.md（不含时间排期）

## 许可

见 LICENSE.md。默认建议 MIT OR Apache-2.0。

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
| 架构 | RISC-V64 主线 + LoongArch64 同源（持续维护，功能按主线/适配阶段推进） | docs/01-architecture.md、docs/08 |
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

前置：Rust 工具链由 `rust-toolchain.toml` 锚定（官方评测 nightly 集合内版本，含
`riscv64gc-unknown-none-elf` 与 `loongarch64-unknown-none` 两个 target），`rustup` 会自动准备；
运行需要 QEMU（版本与评测环境对齐，见 docs/07）。

```bash
# 1) 选架构生成配置（架构是 Kconfig 的 choice，切换要连 defconfig 一起重来）
python tools/kconfig.py --defconfig config/defconfig-riscv64     # 或 defconfig-loongarch64

# 2) 编译检查 / 构建
make check
make build            # 产物 build/<ARCH>/kernel.elf，并校验 ELF 入口地址

# 3) 换架构（三次独立调用，不能合成一条命令；合成命令会跳过 build.mk 并删除配置）
make clean
make ARCH=loongarch64 defconfig
make ARCH=loongarch64 build

# 4) QEMU 运行
make run

# 5) 调整日志级别等选项
python tools/kconfig.py --set LOG_LEVEL_TRACE=y
```

其它目标：`make gdb`（等 GDB 连接）、`make fmt` / `make fmt-check`、`make clean`。
`make test` 尚未接入自动测例（docs/10），接入前该目标明确失败，不代表任何通过。

## 仓库结构（M1）

```
kernel-project/
├── AGENTS.md          # AI 协作治理（源自 ../sdd-governance/AGENTS.md，仓库地图为本仓库实际内容）
├── Makefile / build.mk    # 构建入口与 Kconfig→Cargo feature 转译
├── config/            # Kconfig、defconfig-riscv64 / defconfig-loongarch64
├── linker/            # riscv64.ld（0x80200000）、loongarch64.ld（0x00200000）
├── tools/             # kconfig.py（自研解析器）、check-image.py（映像门禁）
├── src/arch/          # 唯一架构差异入口：riscv64 / loongarch64
├── src/kernel/        # 核心子系统：mm、trap、klib、elf、syscall（M1.5：RV 用户态首闭环；task/sched/fs 待建）
├── docs/              # 设计文档 00–10
└── log/               # boundary / change-record 等证据（见 ../sdd-governance/）
```

## 当前状态表（2026-10-12）

| 能力 | 状态 | 验证证据 |
|---|---|---|
| SDD 治理落地（AGENTS/docs/log） | 已实现 | 仓库文档 |
| Kconfig → build.mk → Cargo feature 构建链 | 已实现，有已知限制 | 见下方已知限制 ③④ |
| 双架构 boot（RV 0x80200000 / LA 0x200000） | **运行验证通过** | QEMU 9.2.1 双架构 M0/M1 启动日志（log/change/） |
| 分页激活 + heap（M1） | **运行验证通过** | 双架构 QEMU 至 "M1 paging/trap foundation OK"（log/change/2026-10-11-M0-qemu-boot-fixes.md 等） |
| RV 用户态 / syscall 闭环（M1.5） | **运行验证通过** | QEMU 输出 "hello from user mode" → "user program exited with code 0"（log/change/2026-10-12-M15-usermode.md）；LA 用户态功能后置（占位 stub，编译通过） |
| 任务 / 调度 / fork | 尚未运行 | 仅设计文档 docs/03 |
| VFS / 文件 / shell | 尚未运行 | 仅设计文档 docs/04 |
| 自动测试入口（make test） | 尚未接入 | 接入前明确失败 |

## 已知限制与待修复（来自 2026-10-12 规划书静态审查 + 本地验证）

| 编号 | 问题 | 确认方式 | 状态 |
|---|---|---|---|
| ① | 多核进入时重复清 BSS 的条件性竞态（BOOT_LOCK 在 .data 只保护自身，迟到核可能重置已初始化全局状态） | 静态审查（规划书）；QEMU -smp 4 实测未触发 | 待修复（P0） |
| ② | LoongArch `CSRWR` 汇编 `in(reg)` 约束：指令会把旧 CSR 值写回源寄存器，连续写两个页大小 CSR 时第二次可能不是预期值 | 静态审查（规划书，对照架构手册）；QEMU 实测 LA M1 通过但未反汇编核实该指令序列 | 待修复/待核实（P0） |
| ③ | `make ARCH=<arch> defconfig clean build` 组合命令失效（跳过 build.mk / 删除配置） | 规划书 `make -n` 复现 | README 已改为三次独立调用（P1） |
| ④ | `tools/kconfig.py` `_Evaluator._atom` 引用不存在的 `self.symbols`，条件表达式求值会 AttributeError；`--menuconfig` 未实现 | 规划书宿主复现 | 待修复（P1） |
| ⑤ | LA 内存仅用低 256MiB 段（分段布局）；RV 按构建配置假定物理内存 | 本地实测/静态 | 已知限制（国赛阶段处理） |
| ⑥ | 用户态前置缺口：trap 用当前 sp（需每任务内核栈）、unmap 后无完整 TLB 协议、连续页分配不复用释放区、自旋锁不屏蔽中断、用户页权限校验不完整 | 静态审查 | 进入 M1.5 前必须收敛 |

> 状态表义务：能力状态必须与「已运行验证 / 仅编译验证 / 尚未运行 / 已知限制 / 后续计划」五类一致；
> 旧版本成功记录（如 M0 早期日志）不得覆盖当前提交的验证结论。

## 许可

见 LICENSE.md（当前为占位说明，团队需选定正式许可；华东往届要求公开代码与开发历史、
文档采用 CC-BY-SA 4.0，本届章程发布后核对）。

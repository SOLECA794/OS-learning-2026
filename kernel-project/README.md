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
make ARCH=riscv64 defconfig     # 或 ARCH=loongarch64

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
RISC-V 有界验收入口：`make test`（启动）、`make test-fault`（受控缺页）、
`make test-process`（完整进程/文件/exec）、`make test-shell`（脚本 shell）、
`make test-vfs`（目录、共享偏移、子目录 exec/argv）、`make test-console`（串口输入）、
`make run-cow`（COW C1–C8）；运行结果保存在
`artifacts/runs/<run-id>/`。`run-shell` 是交互演示入口，不等同于有界自动验收。

## 仓库结构（当前）

```
kernel-project/
├── AGENTS.md          # AI 协作治理（源自 ../sdd-governance/AGENTS.md，仓库地图为本仓库实际内容）
├── Makefile / build.mk    # 构建入口与 Kconfig→Cargo feature 转译
├── config/            # Kconfig、defconfig-riscv64 / defconfig-loongarch64
├── linker/            # riscv64.ld（0x80200000）、loongarch64.ld（0x00200000）
├── tools/             # kconfig.py（自研解析器）、check-image.py（映像门禁）
├── src/arch/          # 唯一架构差异入口：riscv64 / loongarch64
├── src/kernel/        # 核心子系统：mm、trap、elf、task（含 FD 表）、syscall
├── src/fs/            # 最小 VFS：路径解析、打开描述、分层 ramfs、boot 程序源
├── src/klib/          # 内核通用库
├── docs/              # 设计文档 00–10
└── log/               # boundary / change-record 等证据（见 ../sdd-governance/）
```

## 当前状态表（2026-10-14）

按 `docs/08-development-process.md` 的竞赛里程碑口径，**RISC-V 已通过 M2.5 基础文件线验收**：
最小 VFS/ramfs、目录与 FD、控制台输入、脚本 shell、按路径执行 ELF/传参/等待退出。
验收使用自有用户程序，不代表已兼容 Linux 全量文件系统 ABI 或通过 LTP。
RISC-V M2.8 的 COW/回收固定范围已完成：父子和内核写入隔离、`wait4` 失败重试、
物理帧分配故障注入、128 轮重复回收均通过 QEMU 验收。
LoongArch 的已运行验证边界是 M1，后续用户态能力不能由编译通过推断。

| 能力 | 状态 | 验证证据 |
|---|---|---|
| SDD 治理落地（AGENTS/docs/log） | 已实现 | 仓库文档 |
| Kconfig → build.mk → Cargo feature 构建链 | 已实现，有已知限制 | 见下方已知限制 ③④ |
| 双架构 boot（RV 0x80200000 / LA 0x200000） | **运行验证通过** | QEMU 9.2.1 双架构 M0/M1 启动日志（log/change/） |
| 分页激活 + heap（M1） | **运行验证通过** | 双架构 QEMU 至 "M1 paging/trap foundation OK"；页帧回收和临时 MemorySet 映射/回收自检见 `log/change/2026-10-14-M3-frame-reuse-both-arch.md`、`log/change/2026-10-14-M3-memoryset-both-arch.md` |
| RV 用户态 / syscall 闭环（M1.5） | **运行验证通过** | QEMU 输出 "hello from user mode" → "user program exited with code 0"（log/change/2026-10-12-M15-usermode.md）；LA 用户态功能后置（占位 stub，编译通过） |
| RV 进程线（调度、fork/exec/exit/wait） | **运行验证通过**（M2 范围） | `log/change/2026-10-12-W3-context-switch-and-timer.md`、`2026-10-12-W4-process-lifecycle.md`；2026-10-14 shell 回归 |
| RV ramfs/FD/脚本 shell | **运行验证通过**（M2.5 基础范围） | `artifacts/runs/process-20261014-vfs-boundary/`、`shell-20261014-vfs-boundary/` |
| RV 控制台输入 | **QEMU 定向测试通过** | `artifacts/runs/console-20261014-vfs-boundary/`；读者等待期间其它任务仍运行，输入后输出 `INPUT_PASS` |
| RV 最小 VFS / 目录 / mkdirat | **运行验证通过**（M2.5 基础范围） | `log/rfc/M25-vfs/index.md`；`artifacts/runs/vfs-20261014-vfs-boundary/`，V1–V6 / `VFS_EXEC_ARGS_PASS` / `VFS_PASS` |
| RV COW fork 与内核写用户 COW 页 | **M2.8 固定范围运行验证通过** | `artifacts/runs/oom-20261014-m28-complete/`；父子/多代隔离、跨页内核写入失败后重试、131 次 fork 对应 137 次 COW 实际复制 |
| M2.8 资源回收与失败路径 | **定向运行验证通过** | `WAIT-RETRY-PASS`；128 轮回收帧数恒为 1301；520 个子地址空间分配故障点、完整 fork 早期 9 点通过；见 `log/change/2026-10-14-M28-cow-oom.md`。文件 read/getdents64 的 EFAULT 不消费游标亦已验证 |
| M3 首批进程/文件 ABI | **RISC-V 定向运行验证通过** | `wait4` 写回 4 字节；`openat` 四参数；目录 fd 相对路径；`lseek`；`pread64` / `pwrite64`；`fstat` 基础字段；见 `log/rfc/M3-wait4-status/`、`log/rfc/M3-openat-abi/`、`log/rfc/M3-lseek/`、`log/rfc/M3-positioned-io/`、`log/rfc/M3-fstat/` |
| LoongArch 用户态、进程与 COW | **仅编译验证 / 尚未运行** | M1 后用户态适配未落地；LA COW 代码仅编译通过 |
| 有界自动运行入口 | **已接入并运行验证** | `make test`、`make test-fault`、`make test-process`、`make test-shell`、`make test-vfs`、`make test-console`、`make run-cow` |

VFS 当前边界：仅 ramfs，无挂载、删除/重命名、权限模型、`.`/`..` 或真正的进程 cwd；
M3 已把 RISC-V `openat=56` 切换为 Linux 四参数 `(dirfd, path, flags, mode)`：
绝对路径忽略 dirfd；相对路径可从目录 fd 解析；当前 `AT_FDCWD` 暂指向根目录。
旧的三参数私有调用方式已移除，仓库内用户程序均已迁移；见
`log/rfc/M3-openat-abi/index.md`。尚无进程 cwd、权限/umask 或完整 O_* flag 支持。
`mkdirat=34` 接受 `(dirfd, path, mode)`：绝对路径忽略 dirfd，相对路径可从
目录 fd 逐段解析，`AT_FDCWD` 暂指向根目录；权限 mode 暂不实施。
普通文件打开模式错误仍沿用项目原有 EACCES；这些兼容缺口进入 M3，不算 LTP 已通过。
`lseek=62` 已支持普通文件和目录的 `SEEK_SET/CUR/END`；偏移属于共享打开描述，
因此 `dup`/`fork` 共享。控制台返回 `ESPIPE`；`SEEK_DATA`/`SEEK_HOLE` 尚未实现。
`pread64=67` / `pwrite64=68` 已支持 ramfs 普通文件的定位读写，均不改变共享游标；
当前仍沿用单次最多一页的读写上限，`O_APPEND` 下的定位写会追加。
`fstat=80` 已按 RISC-V 128 字节 `struct stat` 写回文件/目录/控制台的类型、
ramfs 文件的 inode 编号与长度。uid/gid、时间戳和权限位暂无模型，不能把这些零值当作真实权限语义。
M3 的 `wait4` ABI 切换已完成：RISC-V 状态写回使用 Linux `int` 的 4 字节宽度，
用户态 canary 测试证明相邻字段不被覆盖，见 `log/rfc/M3-wait4-status/index.md`。
其余 PID 组、选项、rusage 及 LoongArch 用户态等待语义仍待实现。

## 历史问题与当前限制（原 2026-10-12 审查清单）

| 编号 | 问题 | 确认方式 | 状态 |
|---|---|---|---|
| ① | 多核进入时重复清 BSS 的条件性竞态 | `log/change/2026-10-12-W1-p0-p1-fixes.md`；`BOOT_LOCK` 位于 `.data`，唯一主核清 BSS | **已修复**（P0） |
| ② | LoongArch `CSRWR` 旧值覆盖输入寄存器 | 同上；2026-10-14 对现有 LA ELF 反汇编：两个页大小 CSR 分别由 `$a6`、`$a3` 装载 12 后写入 | **已修复并静态核对**（P0）；本次未重新运行 LA QEMU |
| ③ | `make ARCH=<arch> defconfig clean build` 组合命令跳过 build.mk | `Makefile` 已拒绝配置/清理与构建同条命令，快速开始按独立命令执行 | **已修复**（P1） |
| ④ | `tools/kconfig.py` `_Evaluator` 引用错误；`--menuconfig` 缺失 | `log/change/2026-10-12-W1-p0-p1-fixes.md`；当前 `_Evaluator` 使用 `menu.symbols`，`--menuconfig` 有入口 | **已修复**（P1） |
| ⑤ | LA 内存仅用低 256MiB 段（分段布局）；RV 按构建配置假定物理内存 | 本地实测/静态 | 已知限制（国赛阶段处理） |
| ⑥ | 用户态前置缺口：每任务内核栈、TLB 失效、帧复用、锁/中断与用户指针校验 | M1.5–M2.8 记录：每任务内核栈、单核 TLB 刷新、连续帧复用和用户指针逐页 PTE 校验已落地 | **单核范围部分处理**；SMP 锁/中断协议与完整用户指针语义仍待后续阶段 |

> 状态表义务：能力状态必须与「已运行验证 / 仅编译验证 / 尚未运行 / 已知限制 / 后续计划」五类一致；
> 旧版本成功记录（如 M0 早期日志）不得覆盖当前提交的验证结论。

## 许可

见 LICENSE.md（当前为占位说明，团队需选定正式许可；华东往届要求公开代码与开发历史、
文档采用 CC-BY-SA 4.0，本届章程发布后核对）。

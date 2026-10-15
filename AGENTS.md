# AGENTS.md — 仓库级 AI 协作治理文档

本文档面向在本仓库中协作的 AI agent 与开发者。以下规则适用于整个仓库。

## 前置

构建环境：WSL Ubuntu-24.04。在 `kernel-project/` 目录下执行。

首次或 clean 后必须重新生成配置：

```bash
python3 tools/kconfig.py --defconfig config/defconfig-riscv64
```

## 仓库地图

### 常用命令

```bash
make ARCH=riscv64 build           # 构建 kernel.elf
make ARCH=riscv64 test            # 基础回归
make ARCH=riscv64 test-fault      # 缺页故障注入
make ARCH=riscv64 run-cow         # COW 128 轮回收
make ARCH=riscv64 test-shell      # shell 有界回归
make ARCH=riscv64 test-vfs        # VFS 回归
make ARCH=riscv64 test-cow-oom    # COW OOM 520 点注入
```

合并进 master 前必须 6 项全绿。

### 模块地图

| 目录 | 职责 |
|---|---|
| `src/arch/` | 架构抽象：riscv64 / loongarch64，唯一允许架构差异的入口 |
| `src/kernel/mm/` | 帧分配器、页表、MemorySet、内核栈/堆、COW 引用计数 |
| `src/kernel/task/` | TCB、调度上下文、FD 表、FIFO 调度 |
| `src/kernel/syscall/` | Linux UAPI 兼容层：参数校验、copy_from_user、errno |
| `src/kernel/trap.rs` | 架构无关 trap 分发 |
| `src/fs/` | VFS + ramfs |
| `src/klib/` | 自旋锁、日志、同步原语 |
| `user/src/bin/` | 用户态测试程序（shell / uapp-cow / uapp-vfs 等） |

### 架构约定

- 所有跨架构代码通过 `src/arch/` 统一入口。
- 双架构对齐：RV 每固化一个核心里程碑，LA 同步推进，不长期分叉。
- git 操作优先用 Windows 侧（WSL 跨 D 盘 IO 慢）。

## 硬性边界

1. 不启动无界 `make run-shell` 交互版；回归统一走 `test-shell`（pattern "shell: done"）。
2. 不谎报验证：只做了静态检查必须明确说明。
3. 只改任务要求的部分，不无关重构。
4. 仓库含 6 个 submodule，禁止 `git add -A`，必须显式路径 add。
5. 涉及 public API / ABI / owner 边界变化时先上报。

## 验证伦理

- 区分 `agent-run` / `user-run` / `not-run`。
- 运行验证走 `make ARCH=riscv64 <target>`，不用临时命令代替。
- 测试产物在 `artifacts/runs/<run-id>/`，不入版本库。

## 文档维护

- 改代码 → `kernel-project/log/change/<日期>-<描述>.md`
- 设计决策 → `kernel-project/log/rfc/<topic>/index.md`
- 卡死/flaky → `kernel-project/log/incident/<日期>-<简述>.md`
- 合并 commit → 根 `CHANGELOG.md` 顶部加一条
- 里程碑完成 → 根 `ROADMAP.md` 打勾

完整治理规则见 `sdd-governance/README.md`。

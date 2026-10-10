# AGENTS.md — 仓库级 AI 协作治理文档

本文档面向在本仓库中协作的 AI agent 与开发者。除非另有明确说明，以下规则适用于整个仓库。修改本文档需经团队评审。

> 唯一权威来源：`../sdd-governance/AGENTS.md`。本文件是其在 kernel-project 的具体化：仅「仓库地图」一节为本仓库实际内容，其余规则与上游保持一致，修改上游后需同步此文件。

## 前置

如果 `LOCAL.md` 存在，它是开发者个人的额外环境说明，请在读完本文件后再阅读它。本仓库所有 AI 工具（Claude Code / Codex / Cursor / 自建 agent）共用本文件与 `../sdd-governance/skills/` 目录。

## 仓库地图

### 常用命令

```bash
make defconfig       # 用 ARCH 对应的默认配置生成 config/.config
make menuconfig      # 交互配置（依赖 kconfiglib 的 menuconfig，Windows 下用 defconfig 代替）
make check           # 快速编译检查（cargo check，双架构均可）
make build           # 完整构建 → build/<ARCH>/<KERNEL_IMAGE>
make run             # 构建并 QEMU 运行
make gdb             # 构建并等待 GDB 连接
make test            # 测试套件入口（M0 为占位，M1 后接入）
make clean           # 清理构建产物
```

所有构建必须经 `make` 入口。构建由 `config/` 驱动（架构、工具链、feature），直接调用 `cargo` 会缺少 `build.mk` 导出的环境变量与 linker 参数，禁止绕过仓库构建编排。

架构切换：`ARCH=riscv64 make build` / `ARCH=loongarch64 make build`。架构选择会派生 RUST_TARGET、链接基址、linker script，禁止只切 ARCH 而保留另一架构的已解析配置（切 ARCH 时先 `make clean`）。

### 模块地图

| 目录 | 职责 |
|---|---|
| `src/arch/` | 架构抽象与唯一架构差异入口（`riscv64` / `loongarch64`） |
| `src/kernel/` | 核心子系统：mm、task、scheduler、syscall、trap、event、signal、ipc、usync |
| `src/fs/` | VFS + 具体文件系统（tmpfs → ext4） |
| `src/driver/` | 设备驱动框架（FDT 匹配）+ 具体驱动 |
| `src/net/` | 自研网络协议栈（socket → TCP/UDP） |
| `src/klib/` | 通用内核库：分配器接口、锁原语、日志宏、容器工具 |

> 命名说明：`sdd-governance/AGENTS.md` 模板中的 `src/utils/` 在本仓库取 `src/klib/`（与 docs/01-architecture.md §5 一致）。模板为通用占位，本仓库以架构文档为准。

### 架构约定

- 所有跨架构代码通过 `src/arch/mod.rs` 的条件编译入口，禁止直接使用架构私有路径。
- 架构选择在 Rust 侧用 `cfg(feature = "arch-riscv64" / "arch-loongarch64")`，不用 `target_arch`：同一份 feature 名是构建链（Kconfig → build.mk → Cargo）的真相源。
- 新增系统调用：改 `src/kernel/syscall/` 下对应分类文件 + 编号表 + 用户态测试。
- 共享逻辑写一次；跨架构改动必须同时检查两个架构的实现（docs/09 §7）。

## 硬性边界（禁止事项）

1. **不启动任何模拟器/内核后台任务**，包括 `make run`、`make gdb`、`qemu-system-*`，除非用户明确要求运行。
2. **不谎报验证**：不要把未经运行确认的修改描述为"已运行验证"或 "runtime-tested"。只做了静态检查、编译或代码审阅，必须明确说明（标注 `agent-run` / `user-run` / `not-run`）。
3. **不无关重构**：只改任务要求的部分。不"优化"相邻代码、注释、格式；发现无关死代码只提出，不删除。
4. **不回滚用户已有改动**：遇到非自己造成的工作区变更，先读懂并协作，禁止擅自还原。
5. **不执行高风险副作用动作**：不修改镜像、不 patch 启动盘、不写入共享环境，除非用户明确要求并接受变更。
6. **不越权改契约**：涉及 public API、ABI、共享契约、owner 边界变化时，先停止并上报，不得静默替换。
7. **不引入未评审依赖**：本仓库内核主体必须自研。第三方 crate 只允许基础设施类（分配器、容器、寄存器定义、设备树、virtio 寄存器），须在 docs/01 §7 白名单内并评审固定版本。

## 编程原则

### 1. 编码前先思考
- 明确陈述假设，不确定就提问。存在多种理解时列出，不默默选一种。
- 有更简单的方案要提出，必要时提出异议。
- 不清楚就停下来，不要带着困惑写代码。

### 2. 简单优先
- 只写解决问题所需的最少代码，不写推测性功能、不写未被要求的"灵活配置"。
- 单次使用代码不做抽象化；200 行能缩到 50 行就重写。

### 3. 精准修改
- 每一行改动必须直接对应用户请求。删掉自己引入的未使用代码，不删已有死代码。
- 保持原有风格。格式化器产生的无关改动只有在维护风格一致性时才可接受。

### 4. 目标驱动执行
- 每个任务转成可验证目标："修复 bug" → "先写复现测试，再通过测试"；"重构 X" → "重构前后测试都通过"。
- 多步任务列出计划并标注每步的验证内容。

## 内核代码形状约束

这些约束防止 agent 写出"表面能跑、但状态来源和诊断边界不自然"的并发内核代码。违反以下规则属于评审问题，不是风格建议。

1. **单一真相源**：不缓存能从 owning object 直接推导的字段。仅当性能、稳定 snapshot、跨生命周期诊断三类需求成立时才允许派生字段，字段旁必须说明真相源与是否允许 stale。
2. **状态所有权唯一**：一个状态转换只由一个 owner 负责。其它结构持有 token / handle / 弱引用，禁止"task 里一份、辅助对象里又一份"的双重语义。
3. **窄接口优先**：下层只需要唤醒能力/文件能力/任务身份时，不要传完整 `Task`、`File`、私有锁。优先定义窄 ctx / token / owner API。
4. **诊断字段显式标注**：`owner`、`wait_id`、debug label 等只用于日志/排障的字段，必须标注"不参与行为决策"。纯诊断字段不得反向驱动状态机。
5. **断言策略**：表示正确性不变量的轻量检查用 `assert!`；`debug_assert!` 只用于昂贵扫描或 release 不能承受的检查。cleanup/`Drop` 路径先退订、释放、撤销发布，再用断言暴露 bug。
6. **临时桥必须带退出条件**：为阶段迁移、LTP 兼容、ABI 缺口引入的临时字段/fallback/兼容 wrapper，必须注明保留原因、行为边界、移除条件，不能让后续开发者误认为是长期抽象。

## 验证伦理

- 区分三类验证并如实标注：`agent-run`（agent 执行）、`user-run`（用户执行）、`not-run`（未执行）。
- 运行验证优先走仓库测试入口（`make test`），不使用临时手工命令代替。
- 涉及性能结论时，必须记录：固定环境拓扑、输入身份（SHA-256）、测量口径。插件/采样数据不得冒充正式墙钟数据。
- 复现优先：任何结论应能从代码、日志、脚本重新跑出来，而不是依赖对话上下文。

## 工作流总览

按 `../sdd-governance/skills/workflow/SKILL.md` 将开发工作分为三档：

- **Patch**：行为已确定，只产生代码 + 测试 + Git 证据。
- **小迭代**：局部决策/非显然根因/兼容性选择，需要一份 change record（模板见 `../sdd-governance/templates/change-record.md`），同一 Implementation Boundary 内最多 2 个 checkpoint。
- **RFC**：涉及 owner、ABI、共享契约、非平凡并发/生命周期证明、多个 cutover 或目标重协商时，走 `../sdd-governance/templates/rfc.md`，未经评审不得 cutover。

任何任务开工前，若属于小迭代或 RFC，先填写 `../sdd-governance/templates/implementation-boundary.md`：目标、非目标、owner、handoff、failure、cleanup、受保护的 API/ABI、验收标准、验证方式、停止条件。本仓库的 boundary 归档在 `log/boundary/`，change record 归档在 `log/change/`。

## 回复契约（agent 每次交付时）

1. 改了哪些文件，解决什么问题。
2. 采用了什么验证（具体命令与结果；没跑运行验证要写明原因并给出用户可运行的命令）。
3. 涉及日志/测试时，说明预期看到的关键信号。
4. 对行为仍不确定的部分，直接标出剩余假设和下一步观测点。

# SDD 治理体系（Spec-Driven Development Governance）

一套可直接落地的 **规范驱动开发** 治理文件集：先写规范（spec），再让 agent 按规范执行。提炼自 6 个 2026 操作系统大赛一等奖内核项目的实证做法（Anemone、Kairix、KernelX、RyOS、MossOS、CosmOS 的 AGENTS.md / skills / AI 使用声明 / 证据日志）。

## 核心思想

> **AI 当执行者，不当决策者。**

固定范式 = 喂地图（仓库上下文）+ 划边界（禁令护栏）+ 传方法论（skills）+ 锁证据（可复现）+ 管验收（spec 前置）。

## 目录结构

```
sdd-governance/
├── README.md                        # 本文件：总览与落地路径
├── AGENTS.md                        # 主治理文档：仓库地图 + 硬性边界 + 编程原则 + 代码形状约束 + 验证伦理（复制到你的仓库根目录）
└── skills/                          # 方法论技能（放入你的仓库 .agents/skills/ 或 agent 的 skills 目录）
    ├── workflow/SKILL.md            # 任务分级路由：Patch / 小迭代 / RFC
    ├── debug/SKILL.md               # 渐进式调试方法论
    ├── review/SKILL.md              # 代码评审规范（四级问题分级）
    └── evidence/SKILL.md            # 证据纪律与性能分析契约
└── templates/                       # 可填空模板
    ├── implementation-boundary.md   # 任务开工前声明边界
    ├── change-record.md             # 小迭代变更记录
    └── rfc.md                       # RFC（涉及 owner/ABI/契约变更时）
```

## 三档落地路径

| 档位 | 内容 | 成本 | 适用 |
|---|---|---|---|
| **L1 必做** | 复制 `AGENTS.md` 到仓库根目录，按你的项目填命令和模块表 | 半天 | 任何用 AI 写代码的项目 |
| **L2 建议** | 加 `skills/workflow` + `skills/debug`（+ 性能项目再加 `skills/evidence`） | 1-2 天 | 有固定复杂任务的个人/小团队 |
| **L3 全套** | 全部文件 + RFC 流程 + review skill | 1 周+ | 长期多人项目（3 人+、跨数月、提交过千） |

## 使用顺序（一次开发任务的完整流程）

1. 开工前：agent 读 `AGENTS.md`，确认仓库地图和硬性边界。
2. 分类任务：按 `skills/workflow` 判定 Patch / 小迭代 / RFC，小迭代先填 `templates/implementation-boundary.md`。
3. 执行：按编程原则和代码形状约束改代码，验证按 `skills/evidence` 记录证据。
4. 遇到批量失败：按 `skills/debug` 的渐进式循环调试，不逐条硬啃。
5. 收口：变更按 `skills/review` 自查（Apollyon/Keter 级问题不得关闭），记录验证方式与剩余假设。
6. 涉及 owner/ABI/契约变更：走 `templates/rfc.md`，未经评审不得 cutover。

## 证据原则（贯穿全部文件）

- 每个结论必须可脱离对话复现：代码 + 日志 + 脚本，而不是"AI 说的"。
- 区分三类验证：`agent-run`（agent 跑的）、`user-run`（用户跑的）、`not-run`（没跑的）。
- 静态检查通过 ≠ 运行验证通过，回复中必须明确说明验证强度。

---

## 本项目文件系统维护规则（OS-learning 落地）

### 根目录文件分工

| 文件 | 职责 | 读者 |
|---|---|---|
| `README.md` | 仓库总览（外部入口） | 所有人 |
| `ROADMAP.md` | 当前阶段 + 下一阶段（一页状态） | 外部评审 + 队友 |
| `CHANGELOG.md` | 汇总变更（按 commit 倒序，一行摘要 + hash） | 外部 |
| `CONTRIBUTING.md` | 分支/提交/测试命令约定 | 队友 + 外部贡献者 |

### 子目录

| 位置 | 内容 |
|---|---|
| `kernel-project/docs/` | 架构文档 00–12（00 vision / 01 arch / 02 mem / … / 10 test / 11 glossary / 12 test-index） |
| `kernel-project/log/boundary/` | 里程碑边界定义 |
| `kernel-project/log/change/` | 每次改动记录（按日期命名） |
| `kernel-project/log/rfc/` | 设计决策 |
| `kernel-project/log/incident/` | 事故/异常记录（卡死、flaky、build 失败） |

### 更新触发

| 事件 | 更新位置 |
|---|---|
| 合并 commit 进 master | `CHANGELOG.md` 顶部加一条 |
| 完成一个里程碑 | `ROADMAP.md` 已完成打勾 |
| 改代码/修 bug | `log/change/<日期>-<描述>.md` |
| 设计决策 | `log/rfc/<topic>/index.md` |
| 卡死/flaky | `log/incident/<日期>-<简述>.md` |
| 新缩写 | `docs/11-glossary.md` |
| 新测试入口 | `docs/12-test-index.md` |

### 原则

- doc 系统面向**外部理解**，不假设读者看过代码；术语统一查 glossary。
- ROADMAP 已完成项必须能在 git log 找到对应 commit。
- 详细记录在 `log/change/`，根目录 CHANGELOG 只放摘要。
- 禁止提交 `artifacts/runs/` 产物到版本库。

# 08 · 开发流程

## 1. 总则

本项目采用 **Spec-Driven Development（SDD）** 治理：验收标准前置、agent 与人类共用同一套规则、证据可复现。治理细则以 `../sdd-governance/` 为唯一权威（AGENTS.md + skills + templates），本文件只定义项目层约定。

## 2. 开发治理引用

| 场景 | 使用的治理资产 |
|---|---|
| AI agent 协作 | `../sdd-governance/AGENTS.md`（复制到仓库根目录，改仓库地图一节） |
| 任务分级 | `../sdd-governance/skills/workflow/SKILL.md`（Patch/小迭代/RFC） |
| 调试 | `../sdd-governance/skills/debug/SKILL.md`（渐进式失败索引） |
| 评审 | `../sdd-governance/skills/review/SKILL.md`（四级问题分级） |
| 性能/证据 | `../sdd-governance/skills/evidence/SKILL.md`（A/B 纪律） |
| 模板 | `../sdd-governance/templates/`（boundary / change-record / rfc） |

## 3. 任务生命周期

```
1. 分类（Patch / 小迭代 / RFC）→ 小迭代与 RFC 先填 Implementation Boundary
2. 开工：只改 Boundary 声明的范围；接口先行（多组并行时先冻结核心接口）
3. 实现 + 自测（编译 + 定向测例 + 回归集）
4. 收口前：Architecture Friction Scan（无摩擦/仅 Safe 才关闭）
5. 评审（Apollyon/Keter 不合并）→ CI 全绿 → 合并
6. 变更涉及 ABI/契约/owner：必须先过 RFC 评审，未授权不 cutover
```

## 4. 里程碑与验收（不含时间排期）

| 里程碑 | 目标 | 验收标准 |
|---|---|---|
| M0 地基 | 规范落地、Rust 工具链、双架构 boot、最小内核 | 双架构启动到 init；SDD 治理生效 |
| M1 骨架 | mm + task/sched + trap/syscall 骨架 | fork/exec/wait 可跑；核心接口冻结 |
| M2 功能线 | syscall 广度（LTP 分数驱动） | CAgent 通过率达到目标档；ext4 读写可用 |
| M3 性能 | BuildStorm 优化 | 双架构不劣于官方 Linux 基线（证据记录） |
| M4 亮点+实板 | 独门亮点 + VF2/2K1000 | 亮点可演示；实板跑通测试套件 |
| M5 收尾 | 稳定化、文档、答辩材料 | 可复现构建 + 文档齐全 |

## 5. Git 工作流

- 分支：`main`（稳定）+ `dev/<subsystem>` + `<username>/<topic>`。
- 提交信息规范（Conventional Commits）：

```
<type>(<scope>): <subject>

type: feat / fix / perf / docs / refactor / test / chore / build
scope: mm / task / sched / fs / syscall / net / driver / arch / ci / doc
示例: feat(fs): 实现 ext4 只读路径与页缓存回读
     perf(task): 优化唤醒路径避免 thundering herd（A/B 见 log/xxx）
```

- 性能提交必须附证据路径（log/ 或 PR 描述），无证据的 perf 提交不合并。
- 涉及 ABI 语义变更：提交信息标注 `[ABI]`，并链接 RFC。

## 6. 文档纪律

- 设计文档与代码同步更新：改了架构行为，先改 docs 再改代码。
- 新增子系统/接口：在 docs/ 对应章节登记，禁止只写代码不写文档。
- 证据与日志：进 `log/`（带 SHA-256），不混入代码树；原始日志不进公共文档（提炼决定性行）。

## 7. 团队协作约定

- 多组并行时，核心接口（arch 抽象、VFS trait、Task 结构、syscall 签名）冻结变更需评审。
- 每个子系统指定 owner；跨子系统交接必须有明确 handoff（见 Boundary 模板）。
- 每周收口：合并前完成 friction scan；遗留 Euclid 进入跟踪清单。

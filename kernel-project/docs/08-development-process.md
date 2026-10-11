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

按竞赛阶段分层（区域赛主线 → 国赛扩展），与 docs/00 一致：

| 里程碑 | 阶段 | 目标 | 验收标准 |
|---|---|---|---|
| M0 地基 | 区域赛 | 规范落地、Rust 工具链、双架构 boot、最小内核 | 双架构启动到 init；SDD 治理生效 |
| M1 骨架 | 区域赛 | mm + trap/syscall 骨架、分页激活 | 双架构运行到 M1 标记；分页/heap 证据 |
| M1.5 用户态 | 区域赛 | trap 分发、静态 ELF、最小 syscall、用户态入口 | 用户程序输出并退出；非法访问受控 |
| M2 进程线 | 区域赛 | 上下文切换、用户态时钟抢占、eager fork/exec/exit/wait | 无需主动 yield 的任务均前进；父子隔离；回收正确 |
| M2.5 文件线 | 区域赛 | VFS/FD、ramfs、控制台输入、小 shell | 按路径运行程序、传参、等待退出、读写文件 |
| M2.8 深入主题 | 区域赛（条件性） | COW（普通 fork/权限/TLB/回收稳定后进入） | 父写/子写/内核写用户缓冲区/退出回收正确；复制页数对照 |
| M3 兼容线 | 国赛 | syscall 广度（LTP 分数驱动） | CAgent 通过率达到目标档；ext4 读写可用 |
| M4 性能 | 国赛 | BuildStorm 优化 | 双架构不劣于官方 Linux 基线（证据记录） |
| M5 亮点+实板 | 国赛 | 独门亮点 + VF2/2K1000 | 亮点可演示；实板跑通测试套件 |
| M6 收尾 | 区域赛+国赛 | 稳定化、文档、答辩材料 | 可复现构建 + 文档齐全 |

**双架构推进节奏**（替代"每个阶段双架构完全同时完成"）：

- 共享接口（arch 抽象、TrapContext、syscall 签名）必须同时考虑 RV 与 LA，禁止架构私有路径外泄。
- 每周保留双架构编译 + 已有启动用例回归（W1 起，不等到后期）。
- 用户态功能按"主线先行、适配阶段跟进"：先在 RV 跑通主线，再对齐 LA 限定用例；不得把"LA 能启动"写成"LA 已拥有全部用户态能力"。
- 若 RV 主线在检查点仍不稳定，LA 保留在已验证的 boot/mm 边界，延期用户态移植。

## 5. Git 工作流

- 分支：`master`（稳定）+ `dev/<subsystem>` + `<username>/<topic>`。
- **队友分支工作流（固定惯例）**：队友推分支 → 本地拉取建跟踪分支 → 编译 + 双架构 QEMU 验证 → 发现问题则修复 → 合回 master → 合并后重新验证。**验证通过才允许合回**；合回后 master 必须重跑双架构验证。
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
- **能力状态如实**：README 状态表区分「已实现且运行验证 / 仅编译验证 / 尚未运行 / 已知限制 / 后续计划」，旧版本成功记录不得覆盖新版本结论。

## 7. 团队协作约定

- 多组并行时，核心接口（arch 抽象、VFS trait、Task 结构、syscall 签名）冻结变更需评审。
- 每个子系统指定 owner；跨子系统交接必须有明确 handoff（见 Boundary 模板）。
- 每周收口：合并前完成 friction scan；遗留 Euclid 进入跟踪清单。
- 底层启动与用户态入口双人审查：一人实现、一人按架构手册核查寄存器/栈/权限。
- 测试与文档由各模块 owner 自担，禁止把全部验证工作集中给一人。

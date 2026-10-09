# RFC（规范变更提案）

> 涉及 owner、ABI、共享契约、非平凡并发/生命周期证明、probe、多个 cutover 或目标重协商时使用。默认只有本文件（index.md）；`invariants.md`、`implementation.md`、`backgrounds/` 按真实需要追加。
>
> **流程**：Draft → Review → Accepted → Closed（不可重新打开）。Agent 可提交证据与 reduced-target 提案，但不能自行批准。RFC 文本历史由 Git 保存。

## 状态

- 状态：<Draft / Review / Accepted / Closed>
- 日期：<YYYY-MM-DD>
- 作者：<人>
- 评审：<人>

## 1. 目标（Target）

- 要交付的能力（可验证描述）：
- 非目标：
- 依赖的前置项：

## 2. 动机与背景（背景可选）

- 为什么现在做：
- 现状缺口：
- 证据（指向源码/日志/实验，不写"我觉得"）：

## 3. 边界与所有权

- Owner（最终语义）：<子系统>
- Handoff / 交接协议：
- Failure / Cleanup：
- 受保护的 public API / ABI / visible semantics / current contract：
- correctness invariants（不可妥协项）：

## 4. 变更设计

- 关键设计决策与取舍（含被否决的选项）：
- ABI 变化清单（Introduce / Refine / Replace / Remove / Scoped Exception）：
- 临时桥 / probe / 过渡契约（含退出条件）：

## 5. 实施路线

- Stage 划分（每个 stage：目的、前置、受保护边界）：
- Checkpoint / Cutover 计划：
- 验证安排（命令 + 证据 + 验证者）：

## 6. 验收与停止条件

- 验收标准（可执行）：
- 停止条件（何时必须停下上报）：
- 回滚路径：

## 7. 开放问题与已接受限制

- 开放问题（进 register/open-issues）：
- 已接受限制（进 register/current-limitations）：

## 评审结论

- Review 1：<结论 + 证据要求>
- Target Renegotiation（如有）：<原目标 vs 修订目标 + 成本证据>
- Closure：<最终 target、契约变更清单、验证证据、剩余路由>

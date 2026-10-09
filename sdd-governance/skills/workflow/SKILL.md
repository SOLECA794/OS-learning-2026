---
name: development-workflow
description: Use when classifying, planning, documenting, reviewing, promoting, or closing development work across patches, small change records, and RFCs. Enforces minimal artifact selection, Implementation Boundary, cutover honesty, and architecture-friction closeout.
---

# Development Workflow（任务分级路由）

规范驱动开发（SDD）的执行核心：**先分类，再选最小的产物；验收标准前置；未经授权的变更不落地。**

## 1. 分类任务

开发工作只有三档，先判定再动手：

| 档位 | 判定条件 | 需要什么产物 |
|---|---|---|
| **Patch** | 行为已确定，只是实现 | 代码 + 测试 + Git/PR 证据。不需要任何文档 |
| **小迭代** | 局部决策 / 非显然根因 / 兼容性选择 / 可复用调查 / 一次最终局部 cutover | 一份自包含 change record（`templates/change-record.md`），不强制同步周报 |
| **RFC** | owner、handoff、failure、cleanup、ABI、共享契约、非平凡正确性证明、probe、多个 cutover、目标重协商未闭合 | `templates/rfc.md`（默认只有 index 一节，其它页面按真实需要创建） |

判定规则：**能落到 Patch 就不要升档**。文档是负担，不是仪式。不要为了"显得规范"给普通提交加 RFC。

## 2. Implementation Boundary（语义边界）

小迭代和 RFC 开工前，声明语义级边界（不是逐文件清单）：

```
目标：要交付什么能力（可验证的描述）
非目标：明确不做的事
Owner：哪个子系统拥有最终语义
Handoff：跨子系统交接点与协议
Failure：失败路径与回滚策略
Cleanup：资源释放责任
受保护的 public API / ABI / visible semantics / current contract：不得破坏的清单
验收标准：可执行、可验证
验证方式：具体命令与证据
停止条件：何时必须停下来上报
```

- 文件/目录列表只作为非穷举提示；同 owner 内新增文件、行为保持的拆分可由 agent 自然闭合。
- 一旦实现需要改变 target、owner、ABI、契约、验收强度，**必须在完成声明前停止并上报**，不得硬着头皮做完。

## 3. Checkpoint 与 Cutover

- 小迭代默认 **1 个闭包 checkpoint**；确需独立 review/commit/授权停止点时，同一 Boundary 内最多 **2 个 execution checkpoint**。
- CKPT 1 必须独立安全，对受保护的可见语义和 current contract 保持中性。
- CKPT 2 完成整个 target，承担至多一次最终语义/契约 cutover。
- 两个 checkpoint 共用同一份 change record，不拆文档。
- 用户只授权 CKPT 1 时，完成后停止，不自动进入下一阶段。

升级 RFC 的情形（出现任一即升级）：跨 checkpoint 重新解析 target/owner/契约、需要 probe、过渡契约、多个独立 cutover、超过 2 个正式 checkpoint、目标重协商。

## 4. Cutover 诚实性

- Draft/Accepted 的 target 不得提前覆盖 effective contract；只有达到验证和停止条件后才更新 current contract。
- 证据可以来自：原子 change record、RFC closure、Git/PR、按需 transaction。
- 不得为了适配旧文件提示制造不自然的 adapter、重复状态或绕过路径。

## 5. Architecture Friction Scan（收口前必做）

每次 Patch/小迭代/checkpoint/stage/RFC 实现收口前，扫描以下摩擦项：

- 第二份状态真相（同一事实两处保存）
- owner 穿透（跨子系统直接摸内部状态）
- 私有表示泄漏
- 为局部需求扩大 public API
- 调用者/架构/测试特判
- 无退出条件的临时桥
- 隐含的 failure/cleanup 顺序问题
- 无真实义务的新抽象层
- 通过降低 oracle/validation/ABI 诚实性换取"跑通"

结论必须有具体证据（代码路径、状态模型、已接受的下一步），不是"我觉得没问题"。

- 无摩擦或只有 Safe 级：不输出占位结论。
- 有未消除的 Euclid：简短报告证据、模型偏差、影响、最小修正方向。
- 出现 Keter/Apollyon：**立即停止**，不得声明完成或 cutover，报告当前 diff 处置和需要的 owner/RFC/target 决策。

## 6. 停止条件（汇总）

以下情况必须停止并上报，不得继续：

1. 下一步会改变 accepted target / invariant / ABI / owner / validation claim。
2. 复现需要特权镜像、长 QEMU 运行、或不可得的日志，当前证据不足。
3. 要落地一个静默削弱外部行为的兼容桥（除非 ABI 取舍、可观测性、移除条件已文档化）。
4. 目标重协商提案需要批准——agent 可以提交证据和 reduced-target 提案，但不能自行批准。

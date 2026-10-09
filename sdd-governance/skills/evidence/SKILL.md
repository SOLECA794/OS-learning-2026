---
name: evidence-discipline
description: Use when producing performance conclusions, running A/B experiments, interpreting profiling artifacts, or making any claim that must be reproducible. Enforces evidence hierarchy, fixed experiment topology, SHA-256 identity, denominator honesty, and write-back rules.
---

# Evidence Discipline（证据纪律与性能分析契约）

**来源背景**：本技能提炼自一等奖项目的实证做法——RyOS 的"源码审查 → 探针闭合 → 插件动态预算 → 同条件 A/B → 正确性回归"五步链，以及 KernelX qperf 的证据层级与采样模型。

核心原则：**AI 的静态判断并不天然可信。每个重要结论必须能脱离对话，回到代码、日志、插件和脚本复现。**

## 1. 证据层级

任何结论都要能落到证据层，禁止凭空断言：

```
1. 原始数据（日志、trace、采样 bin、A/B 输出）
2. 数据身份（SHA-256、时间戳、构建哈希、输入镜像身份）
3. 分析方法（脚本、插件参数、统计口径）
4. 结论（必须标注依据的证据层级与验证者：agent-run / user-run / not-run）
```

跨档案比较前必须确认可比性：工作量（workload）、架构、CPU 数、采样设置、内核版本、产物身份一致，否则不得直接比较。

## 2. A/B 实验纪律

- **固定拓扑**：每次 A/B 的 QEMU/实机拓扑（CPU 数、内存、设备）必须与记录一致，禁止中途漂移。
- **身份闭合**：配对内核 ELF + 输入盘 + 运行参数三者同时闭合，禁止把新盘路径替换进旧实验并伪称同一实验。
- **口径统一**：正式结论只用正式墙钟/官方口径；插件、采样、探针数据用于定位，不冒充正式结果。
- **长跑前置校准**：公开新盘/新参数前，先做短校准（起止 marker、退出落盘、数据可分类性），通过后才允许数小时长跑，避免结束后发现数据无法分析。
- **对抗审查**：对已有结论主动寻找反例——架构别名、锁外生命周期、工作量分布、插件边界。找不到反例不等于没有。

## 3. 性能分析契约（采样数据）

- **公开两个分母**：`raw samples`（原始）与 `effective kernel-work samples`（剔除入口/噪声后）。百分比只对有效分母计算，同时保留原始百分比供审计。
- **区分 self 与 inclusive**：self 归叶子帧，inclusive 按调用栈包含；inclusive 跨调用者重叠，禁止相加。
- **陷阱识别**：trap 入口指令的样本可能代表其前的用户态驻留或 idle，不是昂贵的 trap 汇编本身。采样不测睡眠锁/设备 I/O 等待，不能单独证明锁竞争或离 CPU 延迟。
- **热点归因**：完整栈优先；区分工作量根因与叶子成本；检查样本是否集中单核；把路径追踪到当前源码中的具体重复操作（分配、清零、拷贝、bitmap 扫描、校验、查找、加锁、提交、回写）。明确结论是"profile 证明的成本"还是"建议的假设"。
- **未解析符号**：`??` ≠ 样本丢失。区分：无符号名 / 折叠丢弃 / 展开提前停止 / 无样本。有原始 IP 时保留地址并用 addr2line/nm 恢复。

## 4. 优化候选准入

只有同时满足以下条件才进入正式代码（缺一不得落地）：

1. 占据主要 Guest 或内核动态预算；
2. 能连接到明确代码机制；
3. 可在不破坏语义的情况下减少完整成本链；
4. 能通过短边界 A/B 与功能回归证明。

缺少主要占比的微调、纯静态猜测、理论收益低于验证风险的候选，不进入正式代码。

## 5. 正确性回归

- 性能改动必须过功能回归，防止"变快但坏了"。
- 双架构项目：RV 收口后只迁移已验证的不变量，LA 架构路径要重新证明，不能把"RV 通过"当 LA 结论。
- 满分/达标后仍需完成剩余审计（如 LA Phase2），"通过"不是性能分析终点。

## 6. 回写规则

- 简单修复：Patch 默认无文档。
- 小语义修复/兼容性 triage/可复用调查：一份 change record（`templates/change-record.md`）。
- 仍活跃的缺陷：写入开放问题登记（register/open-issues）。
- 已接受的分阶段限制：写入 current-limitations。
- 原始日志默认不进公共文档；提炼决定性几行，注明证据日期与验证者。

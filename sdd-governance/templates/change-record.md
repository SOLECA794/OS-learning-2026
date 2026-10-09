# Change Record（小迭代变更记录）

> 小迭代使用；Patch 不需要。同一 Implementation Boundary 内最多 2 个 checkpoint，共用本记录。

## 概览

- 日期：<YYYY-MM-DD>
- 分类：小迭代
- 关联 Boundary：<implementation-boundary 路径或 ID>
- 涉及子系统：<fs / syscall / mm / ...>
- 验证者：<agent-run / user-run / not-run>

## 问题与根因

- 现象/需求：
- 根因（非显然时）：
- 被违反的不变量 / ABI 规则 / 状态所有权规则：

## 改动

- 改了什么（文件级 + 语义级）：
- 决策与理由（为什么这样选，不这样选）：
- 兼容/ABI 取舍（如有）：
- 临时桥 / 退出条件（如有）：

## 验证

| 检查项 | 命令 | 结果 | 验证者 |
|---|---|---|---|
| 编译 |  | PASS/FAIL |  |
| 定向测例 |  | PASS/FAIL |  |
| 回归集 |  | PASS/FAIL |  |
| 性能 A/B（如涉及） |  | 数字 + 口径 |  |

- 失败集 diff：<消失 / 保留 / 新增失败>
- 残余风险与下一步最小验证：

## 收口检查（Architecture Friction Scan）

- 第二份真相源：无 / <证据>
- owner 穿透：无 / <证据>
- 无退出条件的临时桥：无 / <证据>
- 扩大 public API：无 / <证据>
- 降低 ABI/验证诚实性：无 / <证据>
- 结论：<Safe / Euclid（附最小修复方向）/ Keter（停止并上报）>

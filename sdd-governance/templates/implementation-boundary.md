# Implementation Boundary（任务边界声明）

> 小迭代与 RFC 开工前填写；Patch 不需要。填完由 agent 与用户确认后执行。

## 基本信息

- 任务：<一句话描述>
- 分类：<Patch / 小迭代 / RFC>
- 日期：<YYYY-MM-DD>

## 边界

- **目标**：<交付什么能力，可验证描述>
- **非目标**：<明确不做的事，防 scope creep>
- **Owner**：<哪个子系统拥有最终语义>
- **Handoff**：<跨子系统交接点与协议，无则写"无">
- **Failure**：<失败路径与回滚策略>
- **Cleanup**：<资源释放责任归属>

## 受保护契约（不得破坏）

- public API：<列表>
- ABI / visible semantics：<列表>
- current contract：<契约 ID 或描述>
- correctness invariant：<不可妥协项>

## 验收与验证

- **验收标准**：<可执行、可验证>
- **验证方式**：<具体命令与证据，如 make test / 定向测例>
- **验证者**：<agent-run / user-run / not-run>
- **停止条件**：<何时必须停下来上报>

## 路线（小迭代）

- CKPT 1：<独立安全，对受保护语义中性> → 验证：<...>
- CKPT 2（如有）：<完成整个 target，至多一次最终 cutover> → 验证：<...>

## 升级触发

以下任一出现即升级 RFC 并停止：<目标/owner/ABI/契约变化、probe、过渡契约、多个 cutover、目标重协商>

# CONTRIBUTING.md

本文件定义所有贡献者的协作规则。开发治理细则见 docs/08-development-process.md，AI agent 规则见 `../sdd-governance/AGENTS.md`。

## 协作前提

- 所有代码自研。参考开源项目（Linux UAPI、xv6、StarryX、rCore 等）仅用于设计参考，**不得复制代码**；借鉴需在提交信息或文档中注明来源与理由。
- 双架构项目：任何共享逻辑改动必须同时考虑 RISC-V64 与 LoongArch64 实现；只改单架构而不处理另一架构视为不完整提交。
- 接受"完全从 0 自研"的纪律：不引入任何现成内核代码，外部依赖仅限基础 crate（分配器、数据结构、驱动寄存器映射等），且必须在 Cargo.toml 中声明并评审。

## 提交前检查（Checklist）

1. `cargo fmt` 通过。
2. `make check`（编译检查）通过，涉及 feature 时检查 y/n 两侧。
3. 相关测试通过：`make test`（或指定定向测例）。
4. 双架构都编译通过（`ARCH=riscv64` 与 `ARCH=loongarch64`）。
5. 提交信息符合 docs/08-development-process.md 的规范。
6. 若改动涉及 syscall 语义、ABI、锁顺序、状态所有权：补充/更新对应设计文档与注释。
7. 若为性能改动：附带 A/B 证据（见 `../sdd-governance/skills/evidence/SKILL.md`）。

## 分支与 PR 流程

- 长期分支：`main`（稳定，可编译可运行）；`dev/<subsystem>`（子系统开发主线）。
- 个人/实验分支：`<username>/<topic>`。
- PR 合并条件：通过 CI（双架构编译 + 回归）+ 至少 1 名非本人评审 + 提交清单完整。
- 涉及 ABI / 契约 / owner 变更的 PR 必须附带 RFC（见 `../sdd-governance/templates/rfc.md`）。

## 评审要求

- 评审按 `../sdd-governance/skills/review/SKILL.md` 执行：Apollyon/Keter 级问题不得合并；Euclid 级记录并跟进。
- 评审聚焦：架构一致性、并发正确性、ABI 诚实性、资源生命周期、失败路径，不纠结语法。

## 验证伦理（强制）

- 区分 `agent-run` / `user-run` / `not-run` 三种验证，提交说明必须标注。
- 静态检查通过不等于运行验证通过，禁止在提交信息中混淆。
- 性能结论必须可复现：固定拓扑 + 数据身份（SHA-256）+ 口径说明。

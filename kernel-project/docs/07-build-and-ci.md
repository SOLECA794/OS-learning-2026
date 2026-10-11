# 07 · 构建系统与 CI 设计

## 1. 目标

提供可审计、可矩阵化、可复现的构建链：Kconfig 配置 → build.mk 转译 → Cargo feature；CI 覆盖双架构编译与回归；离线依赖保证"从干净环境一键构建"。**构建与运行是两类验证，必须如实区分**（build-only / runtime-tested / not-run）。

## 2. 构建链

```
config/Kconfig（选项声明，bool/string/int/choice，含 help）
  → KCONFIG_CONFIG（默认 config/.config，生成式，不入库）
  → config/config.mk（CONFIG_* 变量导出）
  → build.mk（映射为 RUST_FEATURES / 环境变量 / 工具链参数）
  → Cargo.toml [features] → Rust cfg(feature)
```

规则：

- 用户可选的内核/构建选项进 Kconfig；QEMU 启动值属运行时配置，走脚本而非 feature；CI 覆盖走 CI 策略而非本地默认。
- 架构选择驱动派生值（ARCH_BITS、RUST_TARGET、工具链、条件依赖）：禁止仅切换 ARCH 而保留另一架构的已解析配置。
- 新增编译期选项必须贯通全链：Kconfig 符号 → config.mk → build.mk feature → Cargo feature → CI 矩阵两侧编译验证。

## 3. 顶层 Makefile 目标与编排契约

| 目标 | 作用 | 状态 |
|---|---|---|
| make menuconfig | 交互配置 | **未实现**：当前仅提示并返回失败；如实标注，不虚构交互能力 |
| make check | 快速编译检查（cargo check） | 结果仅代表 build-only，不得宣称运行验证 |
| make build | 完整构建 → build/<ARCH>/<kernel-image> + 映像门禁 | 可复现 |
| make run | 构建并 QEMU 运行 | 运行验证入口 |
| make gdb | 构建并等待 GDB | — |
| make test | 测试套件入口（超时/退出码/原始日志） | 未接入时明确失败，不误报通过 |
| make clean | 清理构建产物 | — |

**编排契约**：

- 顶层编排目标（all/defconfig/clean）与已配置构建目标（build/check/run）分离。**禁止把 `make ARCH=<arch> defconfig clean build` 写成单条组合命令**：`defconfig`/`clean` 在 MAKECMDGOALS 中会跳过 build.mk 导出（build 缺构建变量），且 clean 会删除刚生成的配置。
- 架构切换用三次独立调用：`make clean` → `make ARCH=<arch> defconfig` → `make ARCH=<arch> build`。
- `make clean` 只清理产物；如需彻底清理配置，用显式的独立目标（待补）。
- 环境诊断：显示实际 Rust / QEMU / target 配置与缺失工具，不静默替换版本。
- `make test`：运行有超时、退出码与判定规则的测试，记录原始日志；失败必须有确定的失败结果。

## 4. 离线依赖、工具链与可复现性

- **工具链规则**：Rust 工具链必须落在官方评测 nightly 集合内，并锁定于 `rust-toolchain.toml`（单一真相源）；禁止跟随 stable 漂移，禁止临时切换集合外版本（已验证 1.90+ 存在编译器回归导致 ICE）。**治理文档不写具体版本号**（会过期误导），版本号只存在于 `rust-toolchain.toml`。
- **QEMU 版本与评测环境对齐**（9.2.1）。LoongArch64 需内存 >= 1GiB 且禁用 LSX/LASX 向量特性——此类约束由 build.mk 代码强制（编译参数），不在文档重复规定。
- Cargo.lock 入库；依赖固定版本。
- vendor/ 由工具生成并校验（cargo vendor + checksum），CI 使用离线源构建。
- 每次 `make build` 结束会跑 `tools/check-image.py`，机械校验产物的 ELF `e_machine` 与
  `e_entry` 符合本架构的装载约定（链接基址错了的映像在 QEMU 里表现为"无输出"，
  与串口驱动写错难以区分，必须门禁化，不靠人眼看反汇编）。
- 根目录提供 Dockerfile（或容器化构建说明），保证任何人/任何机器产物一致。
- 证据日志规范：性能/测试记录带 SHA-256 与运行身份（见 sdd-governance/skills/evidence/SKILL.md）。

## 5. CI 矩阵

| Job | 内容 |
|---|---|
| fmt + lint | cargo fmt --check、clippy（若采用） |
| build-rv64 | ARCH=riscv64 build + check |
| build-la64 | ARCH=loongarch64 build + check |
| feature-matrix | 关键 feature 开关两侧编译 |
| test-rv64 / test-la64 | QEMU 内跑基础回归（不启动长任务） |
| repro-check | 干净环境重建 + sha256sum 校验证据 |

CI 纪律：

- CI 不跑数小时长测试（长跑是人工/专用机的事，CI 只做快速门槛）。
- 禁止把 host 绝对路径、私有镜像路径写进 CI 或脚本（可移植占位符）。
- CI 的 build/check 结果一律标注 build-only，不冒充运行验证。

## 6. 验收标准

1. make build 与 make check 在干净环境（容器）可复现。
2. 双架构产物命名规范统一（build/<ARCH>/...）。
3. CI 全绿为合并门槛（双架构编译 + 回归）。
4. 新增 feature 时配置链贯通（含两侧编译验证）。
5. make test 具备超时、退出码、原始日志与真实结果判定；失败不误报通过。
6. README 中的每条构建命令经过实际执行验证；未验证的命令标注 not-run。

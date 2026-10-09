# 07 · 构建系统与 CI 设计

## 1. 目标

提供可审计、可矩阵化、可复现的构建链：Kconfig 配置 → build.mk 转译 → Cargo feature；CI 覆盖双架构编译与回归；离线依赖保证"从干净环境一键构建"。

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

## 3. 顶层 Makefile 目标（起点）

| 目标 | 作用 |
|---|---|
| make menuconfig | 交互配置 |
| make check | 快速编译检查（不链接） |
| make build | 完整构建 → build/<ARCH>/<kernel-image> |
| make run | 构建并 QEMU 运行 |
| make gdb | 构建并等待 GDB |
| make test | 测试套件入口 |
| make clean | 清理构建产物 |

## 4. 离线依赖与可复现性

- Cargo.lock 入库；依赖固定版本。
- vendor/ 由工具生成并校验（cargo vendor + checksum），CI 使用离线源构建。
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

## 6. 验收标准

1. make build 与 make check 在干净环境（容器）可复现。
2. 双架构产物命名规范统一（build/<ARCH>/...）。
3. CI 全绿为合并门槛（双架构编译 + 回归）。
4. 新增 feature 时配置链贯通（含两侧编译验证）。
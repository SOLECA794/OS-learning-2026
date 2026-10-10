# Implementation Boundary · M0 地基（骨架 + 工具链 + 双架构 boot）

> 小迭代与 RFC 开工前填写；Patch 不需要。填完由 agent 与用户确认后执行。

## 基本信息

- 任务：搭建 M0 地基——仓库骨架、Rust 工具链锚定、Kconfig→build.mk→Cargo feature 构建链、RISC-V64 与 LoongArch64 双架构早期 boot 到 `kernel::main`
- 分类：小迭代
- 日期：2026-10-10

## 边界

- **目标**：
  1. `kernel-project/` 成为可构建的 Rust no_std 内核仓库（目录骨架 + 工具链锚定）。
  2. 构建链贯通：`config/Kconfig` → `config/config.mk` → `build.mk` → Cargo feature，支持 `make defconfig/check/build/clean`，`ARCH=riscv64` 与 `ARCH=loongarch64` 双架构产物落在 `build/<ARCH>/`。
  3. 双架构都能从固件入口执行到 `kernel::main`，通过串口输出可观测 banner，随后进入 idle（WFI），secondary hart 安全 park。
  4. SDD 治理在仓库根落地（AGENTS.md + skills + templates 接入 + `log/` 证据目录）。
- **非目标**（防 scope creep）：
  - 不实现 mm / task / sched / syscall / fs / net / driver 任何子系统（M1+ 范围）。
  - 不启用分页（PG/SATP），boot 阶段运行在物理直址模式。
  - 不做 FDT/设备树解析、不接 virtio、不做中断控制器。
  - 不引入任何第三方 crate（M0 只依赖 `core`，确保离线可复现）。
  - 不落地内核堆分配器（M1 随 `klib::kalloc` 一起做）。
  - 不追求串口输入、不实现 `make test` 的真实测例（仅占位入口）。
- **Owner**：`arch`（双架构 boot 与 arch 抽象语义）+ `build`（构建链与配置语义）
- **Handoff**：
  - `arch` → `kernel`：`kernel::main(hartid: usize, dtb_pa: usize)`。M0 只传参不使用 `dtb_pa`（标注为 M1 handoff 点）。
  - `arch` → `klib`：串口写字节能力由 arch 提供 `arch::console::putchar`，klib 只做格式化，不直接碰 MMIO。
  - `build` → `arch`：构建链通过 `cfg(feature = "arch-riscv64" / "arch-loongarch64")` 把架构选择传给 Rust 侧；arch 层不得自行用 `target_arch` 判断。
- **Failure**：
  - 任一架构编译失败 → 停止，不进入下一架构；产物不覆盖上一次成功产物。
  - target 缺失 → Makefile 显式报错并给出 `rustup target add <triple>` 提示，不静默降级。
  - QEMU 不存在 → `make run` 显式报 "qemu not found"，不伪造运行结果。
- **Cleanup**：`make clean` 负责清理 `build/` 与 `target/`；`config/.config` 为生成物不入库（已在 .gitignore）。

## 受保护契约（不得破坏）

- public API：`kernel::main` 签名；`arch` 对外统一 API 的存在性（`arch::console::putchar`、`arch::hartid`、`arch::park`）
- ABI / visible semantics：无（M0 尚无用户态）
- current contract：无（本次为首个 contract，M0 收口后建立）
- correctness invariant：
  - 双架构共享逻辑写一次，架构差异只允许出现在 `src/arch/<arch>/` 内
  - 串口输出必须在 BSS 清零之后、任何格式化之前完成初始化
  - 不得谎报验证：QEMU 未运行的部分必须标注 `not-run`

## 验收与验证

- **验收标准**：
  1. `ARCH=riscv64 make build` 与 `ARCH=loongarch64 make build` 均产出 `build/<ARCH>/<kernel>.elf`
  2. `cargo fmt --check` 通过
  3. 反汇编产物入口地址符合各架构平台约定（RV: 0x80200000；LA: 0x00200000）
  4. 仓库根 AGENTS.md 的仓库地图与实际目录一致
- **验证方式**：
  - `python tools/kconfig.py --defconfig config/defconfig-riscv64 && ARCH=riscv64 make build`
  - `python tools/kconfig.py --defconfig config/defconfig-loongarch64 && ARCH=loongarch64 make build`
  - `cargo fmt --check`
  - QEMU 启动（需用户环境）：见 change record 给出的命令
- **验证者**：编译/静态 = `agent-run`；QEMU 启动 = `not-run`（本机无 QEMU）
- **停止条件**：
  - 需要改变 `kernel::main` 签名或 arch 对外 API 才能继续时
  - 需要引入第三方 crate 才能继续时
  - 双架构出现无法用 arch 层收敛的差异时

## 路线（小迭代）

- CKPT 1：骨架 + 构建链 + 治理落地（对架构语义中性，不含 boot 汇编）→ 验证：双架构 `make check` 通过
- CKPT 2：双架构 boot 汇编 + 串口 + `kernel::main` → 验证：双架构 `make build` 通过 + 入口地址校验

## 升级触发

以下任一出现即升级 RFC 并停止：<目标/owner/ABI/契约变化、probe、过渡契约、多个 cutover、目标重协商>

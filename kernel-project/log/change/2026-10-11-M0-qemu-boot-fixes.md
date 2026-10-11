# Change Record · M0 修复（QEMU 真实启动暴露的 boot 缺陷）

- 日期：2026-10-11
- 分类：小迭代（M0 收口后的缺陷修复）
- 关联 Boundary：`log/boundary/M0-scaffold.md`（M0 边界不变，本次修复在边界内）
- 涉及子系统：arch（riscv64/loongarch64 boot）/ build（Kconfig、build.mk）/ kernel（选主）
- 验证者：`agent-run`（双架构构建 + QEMU 8.2.2 真实启动，SMP=4）

## 背景

M0 change record 将 `make run` 标为 `not-run`（本机无 QEMU）。本次利用 WSL 内已装的
QEMU 8.2.2（满足 Makefile 要求的 >=8.0）执行了真实启动，暴露并修复了三个此前
编译/反汇编/静态检查无法发现的缺陷。

## 问题与根因

| # | 现象 | 根因 | 证据 |
|---|---|---|---|
| 1 | RV 侧 `-smp 4` 启动：OpenSBI 打印后串口无任何内核输出 | boot 汇编 `bnez a0, .Lpark` 假设 OpenSBI 的 bootstrap hart 是 0；实测 OpenSBI 任意选核（本次为 hart 3 / hart 2），非 0 hart 全被 park，含真正拿到 DTB 的 bootstrap | QEMU 日志 `Boot HART ID : 3`；`-smp 1` 时（boot hart=0）输出正常 |
| 2 | LA 侧任何配置启动即报错退出 | `qemu-system-loongarch64: ram_size must be greater than 1G`——LA QEMU virt 机器强制内存 >1G，而 `CONFIG_QEMU_MEMORY_MB=256` | 直接报错；`-m 1G` 后正常 |
| 3 | LA 侧 `-smp 4` 旧代码输出 hartid=0，掩盖了与 RV 相同的问题 | LA boot 汇编同样 `bnez $r12, .Lpark`（假设 core 0）；实测 LA QEMU `-kernel` 模式**也引导全部核**（修复后实测 hartid=2 成为 bootstrap），只是旧代码把非 0 核 park 后看起来"只引导主核" | 修复后 `-smp 4` 输出 `hartid=2` |

## 改动

### 语义级

1. **选主从"汇编假设 hart 0"改为"Rust 原子锁"**：`kernel::BOOT_LOCK`（`AtomicBool`，
   `#[link_section = ".data"]`，初值随镜像加载、不在 BSS 清零范围内），`kernel_main`
   入口 `swap(true, AcqRel)`，失败者 `arch::park()`。
2. **boot 栈从"单块 64K"改为"per-hart 数组"**：每核 8K、共 8 核（`BOOT_STACK_SIZE=8K`、
   `BOOT_HART_COUNT=8`，链接脚本常量），汇编按 hartid 取栈顶，全部核进入 Rust 后由
   原子锁选主；`hartid >= 8` 的核直接停机（M0 明确不支持）。
3. **BSS 清零改为幂等**：所有核都执行清零（写 0 无副作用），顺序不再依赖"只有一个核进"。
4. **LA 内存下限**：`CONFIG_QEMU_MEMORY_MB` 默认 256 → 1024；build.mk 在 LA 分支增加
   `<1024` 的 `$(error)` 检查（在 build 阶段拒绝，而非 make run 时才暴露）。

### 为什么选主放 Rust 而不放汇编

LLVM 对 `global_asm!` 内联汇编的指令可用性检查与 Rust 生成代码不一致：target
`riscv64gc` 的 cfg 明确含 `zaamo/zalrsc/zmmul`，但 `global_asm` 里的 `amoswap.w`、
`mul` 分别被拒（`requires 'Zaamo'` / `requires 'Zmmul'`），而 Rust 的 `AtomicBool`
生成代码可正常编译。规避方式：
- 汇编内仅使用基础指令（乘法用 `slli` 移位替代：8K = 2^13）；
- 原子选主移入 Rust（生成代码路径不受此检查影响）。

### 文件级

- `src/kernel/mod.rs`：新增 `BOOT_LOCK` + `kernel_main` 选主分支
- `src/arch/{riscv64,loongarch64}/boot.rs`：per-hart 栈计算（移位替代乘法）、幂等清 BSS、全部核进入 Rust
- `linker/{riscv64,loongarch64}.ld`：`BOOT_STACK_SIZE=8K`、`BOOT_HART_COUNT=8`，boot 栈数组
- `config/Kconfig`：`QEMU_MEMORY_MB` 默认 1024，help 注明 LA 下限
- `config/defconfig-{riscv64,loongarch64}`：`CONFIG_QEMU_MEMORY_MB=1024`
- `build.mk`：LA 分支内存下限 `$(error)` 检查

## 验证

| 检查项 | 命令 | 结果 | 验证者 |
|---|---|---|---|
| RV 构建 | `cargo build --target riscv64gc-unknown-none-elf --release --features arch-riscv64,log-level-info` | PASS | agent-run |
| LA 构建 | `cargo build --target loongarch64-unknown-none --release --features arch-loongarch64,log-level-info` | PASS | agent-run |
| 入口校验（双架构） | `tools/check-image.py` | PASS（e_entry = 0x80200000 / 0x200000） | agent-run |
| 格式 | `cargo fmt --check` | PASS | agent-run |
| **RV QEMU 真实启动（SMP=4）** | `qemu-system-riscv64 -machine virt -kernel build/riscv64/kernel.elf -smp 4 -m 1G -nographic -serial mon:stdio -bios default`（QEMU 8.2.2） | **PASS：`M0 boot OK`，hartid=2** | agent-run |
| **LA QEMU 真实启动（SMP=4）** | `qemu-system-loongarch64 -machine virt -kernel build/loongarch64/kernel.elf -smp 4 -m 1G -nographic -serial mon:stdio` | **PASS：`M0 boot OK`，hartid=2** | agent-run |
| 内存下限拦截 | `CONFIG_QEMU_MEMORY_MB=256` 时 make build（LA） | PASS（按预期 `$(error)` 退出） | agent-run |

- 失败集 diff：M0 无既有失败集；新增失败（RV SMP 无输出、LA 内存报错）均已闭合。
- 环境注记：本次验证在 WSL Ubuntu-24.04 的 QEMU 8.2.2 执行；官方评测环境为
  `zhouzhouyi/os-contest:20260510`（内置 QEMU 9.2.1），M1 起应在该镜像内做最终验收。

## 残余风险（更新）

1. ~~RV SMP>1 无输出~~（已修复）；`BOOT_HART_COUNT=8` 的硬上限是 M1 `setup_all_cores`
   的输入，届时改为动态/更大上限。
2. LA UART 访存属性（`CRMD.DATM/DATF`）：本次 QEMU 8.2.2 实测**输出正常**（此前头号
   嫌疑未被触发）；实板与 QEMU 9.2.1 仍需 M1 建 DMW/页表时显式接管。
3. BSS 仍为空（无 `static mut`），清零循环的运行时正确性仍未真正覆盖；M1 引入静态结构后生效。
4. RV 依赖 OpenSBI legacy console_putchar（QEMU 8.2.2 OpenSBI v1.3 实测支持）；M1 换 DBCN。
5. `dtb_pa` LA 恒 0（实测输出 `dtb_pa=0x0` 符合预期）；M1 接入 FDT 解析。

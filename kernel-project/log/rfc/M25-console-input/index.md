# RFC：M2.5 RISC-V QEMU 控制台输入

## 状态

- 状态：Closed（RISC-V QEMU 控制台输入目标；完整 M2.5 仍未关闭）
- 日期：2026-10-14
- 作者：Codex
- 评审：项目维护者

## 1. 目标（Target）

- 要交付的能力：RISC-V QEMU virt 上用户程序可通过 `read(fd=0)` 等待并取得串口输入；返回实际字节数，暂时无数据不返回 EOF。
- 非目标：LoongArch 用户态启动、规范终端/行编辑、信号、多个 tty、文件系统重构。
- 依赖的前置项：现有 RISC-V 用户态、syscall FD 表、调度器、trap 入口；QEMU 运行器须可重复地注入 stdin。

## 2. 动机与背景

- 为什么现在做：M2.5 的 shell 目前依赖脚本，缺少交互输入。
- 原状缺口：`src/kernel/syscall/fd.rs::sys_read` 将 Console 当 EOF 返回 0；`src/arch/riscv64/console.rs` 仅支持输出；`src/kernel/trap.rs` 仅处理时钟中断；`src/kernel/task/mod.rs` 的等待路径仅服务 wait4。本轮已写入候选修复，但尚无运行证据。
- 证据：上述源码，以及 `README.md` 的 M2.5 状态。RISC-V SBI 规范有 legacy console getchar，但其无数据返回值不能代替阻塞 read 的等待/唤醒协议；QEMU virt 提供 UART 与 PLIC。硬件路径仍须以本项目镜像的实测为准。

## 3. 边界与所有权

- Owner：syscall 层拥有用户可见 read 语义；控制台驱动拥有接收队列；task 层唯一拥有 Ready/Waiting 转换；arch 层仅处理 IRQ 和寄存器差异。
- Handoff：IRQ/驱动接收字节并通知等待者；task 层将“检查队列为空 + 登记等待者 + 切换任务”组成不丢唤醒的协议；syscall 唤醒后重试取字节并安全写回用户缓冲区。
- Failure / Cleanup：坏 FD/指针返回错误；队列满时丢弃新字节并累计计数。M2 单核/单线程没有并发取消，唤醒时清除 WaitingConsole；线程/信号/跨核关闭进入目标前需补取消协议。失败不得把“当前没有数据”报告为 EOF。
- 受保护契约：现有文件 FD 读写、日志输出、用户 ABI、双架构编译边界和既有回归测试。
- correctness invariants：无丢唤醒；锁顺序固定；IRQ 路径不分配内存、不拷贝用户地址、不持锁调度；读取至少一个字节后允许短读；`len=0` 不睡眠。

## 4. 变更设计

- 首选路径：RISC-V QEMU virt 的 UART RX + PLIC 外部中断，驱动持有有界 RX 环形队列，任务层提供窄的等待/唤醒接口。具体 UART/PLIC 地址与 IRQ 由平台描述确认，避免将板卡常量混入通用 syscall。
- 不采用“在 syscall 中循环 SBI getchar”：当前内核 trap 期间不抢占，会阻塞其他任务；SBI 无数据值也不是 EOF。
- 不采用“没有字节就返回 0”：0 对用户程序是 EOF，会提前结束 shell。
- ABI 变化：Refine `read` 对 Console 的可见语义（由固定 EOF 改为可阻塞输入）；文件 FD 语义不变。架构/任务内部接口为 Introduce，评审后定名。
- 过渡契约：不得出现只返回 EAGAIN 或只支持“先输入后读取”却宣称 M2.5 完成的中间 cutover。LoongArch 用户态输入不在本目标内，保留受控的未支持边界。

## 5. 实施路线

- Stage 1：确认容器 QEMU 参数、设备映射和可注入 stdin 的仓库测试入口；确定 IRQ 编号、队列容量与溢出策略。仅设计/测试准备，无 ABI cutover。
- Stage 2：RISC-V 平台 UART/PLIC RX 与 trap 分发、队列；任务层原子等待/唤醒；保持旧 syscall 行为，独立编译与驱动级验证。
- Stage 3：一次性将 Console `read` 切换至阻塞读取，添加用户态测试，双架构构建与现有回归验证；运行验证通过后才更新 README 的 current contract。
- 验证安排：所有构建走容器内 `make`；运行用仓库测试入口，并由用户明确授权启动模拟器后执行。证据记录命令、镜像、退出码、关键日志。

## 6. 验收与停止条件

- 验收：读等待后输入、同批字节的第二次读取、短读、零长度、坏 FD/指针、文件读写与既有 shell/调度回归均通过；两个架构均可构建。无输入等待时其他就绪任务仍运行。UART 分批交付时第二次读取可以再次等待，不能据此声称所有字节已先于 read 到达。
- 停止：RFC 未评审接受；设备/中断拓扑不符；等待/唤醒原子性不能证明；需要更改 LoongArch 用户态目标或 Linux ABI；未经授权需启动 QEMU。
- 回滚：cutover 前保持旧实现；cutover 后若验证失败，恢复该阶段的 syscall/驱动/任务代码，不改动已有用户工作区文件。

## 7. 开放问题与已接受限制

- 已定：当前 `build.mk` 固定 `-machine virt`、未启用 AIA，因此平台层暂用 QEMU `virt.c` / `virt.h` 定义的 UART0 `0x1000_0000`、IRQ 10、PLIC `0x0c00_0000`，hart 0 的 S context 为 1；切换机器或增加 AIA 时必须从平台描述重新选择。RX 队列容量 256，满时丢弃新字节并计数，避免 IRQ 中分配和日志递归。
- 已定：单 hart syscall 中 SIE=0，`task` 持 SCHED 锁重查 RX 队列后登记 WaitingConsole；UART IRQ 入队后唤醒该状态，阻塞者重试。只有控制台等待者且无 ready 时，在当前任务内核栈上短暂打开 SIE 执行 WFI；周期定时器覆盖检查 ready 与 WFI 之间的竞态。多核前须重新审计锁顺序和等待原子性。
- 已接受限制：仅 RISC-V QEMU virt 运行时；不做行规程、回显或信号语义；LoongArch 本阶段只要求构建不退化。

## 评审结论

- Review 1：用户在阅读 Draft 后要求“继续推进进度向下实现”（2026-10-14）；按此接受原目标和分阶段路线。用户随后明确授权在比赛镜像中运行 QEMU 控制台与现有回归测试。
- Stage 1：`tools/run_qemu.py` 新增可选 fixture 注入：检测串口标记后写入 QEMU stdin，默认继续使用 DEVNULL；单次输入限制 4096 字节并拒绝 QEMU monitor 转义键 Ctrl-A。设备拓扑与运行效果尚未验证。
- Stage 2（部分，静态检查）：RISC-V 平台层登记设备地址与最小 MMIO 映射；UART RX IRQ 搬入固定队列，PLIC 只开启 UART0，trap 只接受 SEI code 9。容器 `zhouzhouyi/os-contest:20260510` 中 `make ARCH=riscv64 check` 与 `make ARCH=riscv64 build` 成功，ELF 入口 `0x80200000`；未运行 QEMU，设备和唤醒行为仍未验证。
- Stage 3（运行通过，尚未关闭）：task 增加 WaitingConsole/IRQ 唤醒与可中断 idle；ConsoleInput 的 read 在预备目的页后阻塞取字节，ConsoleOutput 保持旧 EOF。比赛镜像 `zhouzhouyi/os-contest:20260510` 中 `make ARCH=riscv64 test-console RUN_ID=20261014-console-pty CONSOLE_TIMEOUT=20` 通过；结果在 `artifacts/runs/console-20261014-console-pty/`。测试在一次等待后注入 `abc\nxyz\n`，日志顺序为等待、`INPUT_FIRST_DONE`、`INPUT_PASS`，第二次读取前未再进入等待。运行器改用 PTY 注入 fixture，避免管道输入仅在 EOF 交付且触发 QEMU 退出。shell、COW、fault、boot 回归分别保存在 `artifacts/runs/shell-20261014-console-regression/`、`artifacts/runs/20261014-console-cow/`、`artifacts/runs/fault-20261014-console-regression/`、`artifacts/runs/20261014-console-regression/`，均 PASS；`make fmt-check` 通过。此前隔离临时副本的 `make ARCH=loongarch64 check` 成功。
- Closure（2026-10-14）：后续定向测试暴露 bootstrap hart 可为 0..3，但 PLIC 原来固定配置 hart 0 的 S context，导致输入中断随机发给停驻 hart。现按实际 bootstrap hart 计算 PLIC context，并映射 boot 栈支持的 8 个 hart 的 S context 页。`artifacts/runs/console-20261014-console-hart-{1..5}/` 连续五次 PASS，hartid 分别为 0/3/1/3/2；读者等待先于 `INPUT_OTHER_TASK`，然后 idle 与输入完成标记出现。撤回诊断性 QEMU 参数后的 `console-20261014-console-reverted-runner` 再次 PASS；最终代码 `make fmt-check`、shell QEMU 回归、隔离 LoongArch `make check` 通过。原有 COW/fault/boot 回归见 Stage 3；完整 M2.5 仍需独立 VFS 目标。

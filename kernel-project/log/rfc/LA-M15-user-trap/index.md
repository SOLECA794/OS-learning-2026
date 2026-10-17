# RFC：LoongArch M1.5 用户态与 trap 闭环

## 状态

- 状态：Closed（LoongArch M1.5 单任务目标已运行验收；M2/M2.8 仍待推进）
- 日期：2026-10-15
- 实施边界：`log/boundary/LA-M15-user-trap.md`

## 目标与取舍

现有 LA `restore_from` 仅停驻，`init_user_legacy` 误用 RV ELF，trap 入口直接沿用发生异常时的栈。这三个缺口必须一起关闭，才能声称 LA M1.5，而不是只修改 ELF 机器号或打印标记。

本轮使用原生 LA 静态用户程序和单任务内核栈，内核态只处理受控的 `write/exit`；M2 task/FD/调度 owner 不在本轮切换。其退出条件是共享 task 适配到 LA，单任务桥随之删除。

## 正确性约束

1. 进入 PLV3 前，内核栈已分配，现场在该栈顶，异常入口可以从 CSR scratch 中取回栈顶而不碰用户栈。
2. 用户寄存器必须在被 Rust 调用覆盖前保存；异常返回恢复现场后 `ertn` 进入 `PRMD.PPLV` 指定的权限。
3. 用户指针只能通过当前用户页表逐页校验后复制；不能直接信任裸指针。
4. `write/exit` 使用现有 Linux 编号和参数位置；未知 syscall 保持 `ENOSYS`。
5. QEMU 用户态标记须由真实用户指令和 syscall 产生，不能由内核启动日志伪造。

## 实施/验收

- Stage 1：LA 用户 ELF 构建与 ELF machine 校验；双架构构建。
- Stage 2：LA trap 保存/恢复与单任务 syscall 桥；LoongArch `test-user` 运行，RISC-V 启动回归。
- 运行证据记录 ELF SHA-256、QEMU 版本、标记与失败路径。LoongArch M2/M2.8/M3 仍另立检查点，不在本 RFC 的 Closed 范围。

## 评审

用户明确要求追平 LA 并同意有界 QEMU；按上述最小 M1.5 检查点实施。若目标扩展至 M2 task owner，需新边界和评审。

## 实现与验证（agent-run，2026-10-15）

- `user/loongarch64` 构建原生 `EM_LOONGARCH=258` 静态用户 ELF；ELF 解析器按当前架构拒绝另一架构机器号。
- LA trap 的 `SAVE0` 在用户态持内核栈顶；入口先换栈再保存通用寄存器，内核来源保持原栈；恢复路径写回 `PRMD/ERA` 后 `ertn`。
- 单任务临时桥持有用户 `MemorySet`，`write` 逐页校验后复制用户数据，`exit` 记录退出码并停驻。未知号返回 ENOSYS。该桥的退出条件是 LA 共享 task 适配。
- 比赛镜像：隔离副本 `make ARCH=loongarch64 test-user RUN_ID=20261015-la-m15-errno` PASS，日志在 `artifacts/runs/la-user-20261015-la-m15-errno/`；QEMU 10.0.2、内核 ELF SHA-256 `662e50b4f311f8ac5abaf1394d68628d4af3948a25fef5ba10a033ff19ab7f7b`。串口含 `LA_USER_WRITE_PASS` 和 `LA_USER_EXIT_PASS code=0`，用户程序还断言空指针写入返回 EFAULT、未知号返回 ENOSYS。
- 比赛镜像：`make ARCH=riscv64 test-vfs RUN_ID=20261015-la-m15-rv-regress` PASS，原有 VFS 用户态功能未退化。
- 仍未验收：LA 进程切换、时钟中断、fork/exec/wait、FD/文件 syscall 和 COW。不能把此 M1.5 闭环描述为两架构已同进度。

架构依据：[LoongArch ELF ABI](https://github.com/loongson/la-abi-specs/blob/release/laelf.adoc)
（`e_machine=258`）、[LoongArch 基本架构手册](https://loongson.github.io/LoongArch-Documentation/LoongArch-Vol1-EN.html)
（SAVE0、PRMD/ERTN 异常返回）。

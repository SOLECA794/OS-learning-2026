# Implementation Boundary：M2.5 控制台输入

## 基本信息

- 任务：使 RISC-V QEMU `read` 控制台能够等待并读取串口输入。
- 分类：RFC（系统调用可见语义、设备与调度器交接、等待/唤醒正确性）。
- 日期：2026-10-14。

## 边界

- **目标**：在 RISC-V QEMU virt 上，`read(fd=0, buf, len)` 对有效输入返回实际字节数；暂时无输入时等待，收到输入后可继续执行；`len=0` 返回 0。
- **非目标**：LoongArch 用户态运行时、终端行规程、信号中断、多个 tty、完整 VFS。
- **Owner**：`kernel::syscall::fd` 拥有 read 返回语义；控制台驱动拥有 RX 字节队列；`kernel::task` 拥有任务状态和唤醒。
- **Handoff**：驱动仅发布“有字节可取”的事件；任务层原子地登记等待者并睡眠；syscall 从队列取字节后拷贝到用户空间。中断处理不得持有任务锁做用户拷贝。
- **Failure**：非法 fd 为 EBADF、非法用户缓冲区为 EFAULT；设备未就绪不得谎报 EOF。驱动初始化失败或唤醒不变量不能证明时停止 cutover。
- **Cleanup**：M2 单核/单线程下阻塞任务不能并发执行 exit/exec/close；IRQ 唤醒时任务层清除 WaitingConsole 状态，队列由控制台 owner 管理。引入线程/信号/跨核关闭前必须补取消等待协议。

## 受保护契约（不得破坏）

- public API：`arch` 条件编译入口和现有日志输出钩子。
- ABI / visible semantics：文件 `read/write`、FD 表、Linux 风格 syscall 返回值、现有 shell 与 COW 测试。
- current contract：RISC-V QEMU virt 的控制台 `read` 已切换为等待串口输入；LoongArch 用户态输入不在本目标内。
- correctness invariant：不会丢唤醒；不会在持锁状态睡眠；不会从中断上下文访问用户指针；无数据不能当成 EOF。

## 验收与验证

- **验收标准**：定向用户态测试覆盖等待后输入、同批字节的第二次读取、短读、`len=0`、无效指针/FD，并证明读者等待期间其他就绪任务可运行，回归既有文件读写和调度测试。
- **验证方式**：容器内通过 `make` 双架构构建；在用户明确授权运行后用仓库 QEMU 测试入口注入串口字节并留存日志。
- **验证者**：agent-run；比赛镜像中 `make ARCH=riscv64 test-console RUN_ID=20261014-console-hart-N CONSOLE_TIMEOUT=10` 连续五次通过（N=1..5，bootstrap hart 覆盖 0/1/2/3）；最终配置 `console-20261014-console-reverted-runner` 再次通过，shell/COW/fault/boot 回归通过。LoongArch 在隔离副本中 `make clean`、`make ARCH=loongarch64 defconfig`、`make ARCH=loongarch64 check` 通过。日志顺序为读者等待、`INPUT_OTHER_TASK`、idle、`INPUT_FIRST_DONE`、`INPUT_PASS`；允许 UART 分批交付而再次等待。
- **停止条件**：RFC 未评审接受、需改变 ABI/owner/目标、无法证明睡眠唤醒原子性、或运行需未经授权启动 QEMU。

## 路线

- 设计与评审：见 `log/rfc/M25-console-input/index.md`。RISC-V QEMU 定向运行已验证；完整 M2.5 的 VFS 是独立目标。

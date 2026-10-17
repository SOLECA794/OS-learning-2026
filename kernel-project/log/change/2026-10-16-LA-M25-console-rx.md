# LoongArch QEMU virt 串口输入唤醒

- 日期：2026-10-16
- 范围：仅 LoongArch QEMU virt 的 UART RX；不包含实板中断控制器和 SMP。
- 实现：增加 PCH PIC/EXTIOI 外部中断路由与确认、UART RX 固定环形队列、阻塞 stdin 读者等待及 IRQ 唤醒；无就绪任务时在可中断 idle 中等待。
- 用户验收：新增 `la-console`，fork 子任务阻塞读取 fixture 的 `abc\n`，读到后退出 7，父任务 wait4 检查状态并输出 `LA_CONSOLE_PASS`。
- 修正：验收汇编使用 `la.local` 计算本地 rodata 地址，避免父任务 `la` 生成的空指针；runner 等待最终 `LA_USER_EXIT_PASS code=0`，避免在中间标记后过早终止 QEMU。
- 验收：`make ARCH=loongarch64 test-la-console` 通过；日志顺序包含 `LA_CONSOLE_WAIT_READY`、读者 waiting、idle、waking、`LA_CONSOLE_PASS`、`LA_USER_EXIT_PASS code=0`。
- 兼容边界：RISC-V UART/SBI/PLIC 路径不改；LoongArch 外部 IRQ handler 仅处理已配置 UART 中断，未知位仍进入 fatal 路径。

# M1 内核分页激活边界

## 目标

激活现有内核 `MemorySet`：RISC-V64 使用 Sv39，LoongArch64 配置三级页表与软件 TLB refill，并保证切换后的串口和电源 MMIO 可访问。

## 非目标

- 不切换到高半内核地址，不实现 ASID、用户地址空间切换或缺页按需分配。
- 不改变普通 trap frame ABI。
- 不启动 secondary core。

## Owner 与 Handoff

- `kernel/mm` 拥有映射内容和激活时机。
- `arch/*/mm` 拥有根页表寄存器、TLB 操作与 PTE 内存类型编码。
- LoongArch TLB refill 入口只装填现有 PTE；非法/缺失映射最终进入普通异常路径。

## Failure / Cleanup

分页切换不可回滚；因此切换前必须完整映射当前代码、栈、BSS、页表页和平台 MMIO。失败条件是双架构不能编译、架构指令/CSR 编码不确定，或静态映射不完整。

## 验收与验证

- `make all` 双架构通过，ELF 入口不变。
- 静态检查切换点前后的所有必需地址均有身份映射。
- 未经用户明确要求不启动 QEMU；因此运行期成功日志不作为本轮已完成证据。

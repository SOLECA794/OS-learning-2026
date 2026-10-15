# P0: brk(2) 动态堆系统调用

## 变更

- `MemorySet` 新增 `heap_base=0x2000_0000` 和 `heap_brk` 字段
- 实现 `brk(new_brk)`：扩展逐页分配 RW 用户页，收缩逐页 unmap 释放
- syscall 分发 `SYSCALL_BRK=214`，ENOMEM=-12
- 新增用户态验收程序 `uapp-brk`（B1-B5：初始 break/扩展/写入读回/收缩/收缩后可写）
- 注册 `make test-brk` 回归目标

## 地址选择

堆起始 0x2000_0000，避开：
- 0x1000_0000 = QEMU UART0 MMIO（曾误选此地址导致 AlreadyMapped）
- 0x0c00_0000 = PLIC

## 验证

- `make test-brk` PASS（B1-B5 全过）
- 6 项原有回归全绿（test/test-fault/run-cow/test-shell/test-vfs/test-cow-oom）

# 修正双架构 munmap 范围处理

- 日期：2026-10-16
- 问题：旧 `MemorySet::munmap(addr, len)` 忽略 `len`，只尝试删除恰好以 `addr` 开始的整个映射区；`mmap_anon` 还把向上取整后的页数误当字节数，令多页请求只映射一页。
- 实现：修正 mmap 长度按页大小还原并检查零长度/溢出；munmap 校验地址页对齐和非零长度，对长度向上覆盖的页范围解除用户映射，按需要拆分左/右保留区域，释放对应 COW/独占页引用并刷新本核 TLB。无映射洞按 munmap 语义忽略。同步修正 brk/mmap/munmap 与 pipe 写入只读端重复取反 errno 的问题。
- 回归：扩展 LoongArch 与 RISC-V 用户态匿名 mmap 用例，验证零长度/非对齐参数返回 EINVAL、中间页部分解除、邻页仍可读写、洞页写 syscall 返回 EFAULT，以及覆盖洞的最终清理。
- 验收：agent-run，比赛镜像 `make ARCH=loongarch64 test-la-memory RUN_ID=20261016-la-munmap-pass LA_MEMORY_TIMEOUT=15` 通过（`LA_MEMORY_PASS`、退出 0）；`make ARCH=riscv64 test-mmap RUN_ID=20261016-rv-munmap-pass` 通过（`MMAP-PASS`）。
- 边界：沿用当前单核 TLB 刷新模型；SMP TLB shootdown、多进程并发解除同一地址空间仍未支持。

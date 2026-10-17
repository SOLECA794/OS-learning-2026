# LoongArch brk 与匿名映射 syscall 接入

- 日期：2026-10-16；分类：小迭代；关联边界：`log/boundary/LA-user-memory-syscalls.md`。

开放 LoongArch 的 `brk/mmap/munmap` 分发，继续复用当前任务的公共 MemorySet。原生用户程序
验证初始 break、堆扩展与收缩、堆页读写、匿名页映射读写及 munmap。系统调用参数行为沿用
既有公共实现：mmap 当前仅按长度创建匿名 RW 页，不声称 Linux 全量 flags 或文件映射支持。

## 验证

- agent-run：比赛镜像 LoongArch `test-la-memory RUN_ID=20261016-la-memory-b` PASS，`LA_MEMORY_PASS` 和退出 0。
- agent-run：RISC-V `test-brk RUN_ID=20261016-la-memory-rv-brk` PASS。
- agent-run：RISC-V `test-mmap RUN_ID=20261016-la-memory-rv-mmap` PASS。
- agent-run：`make ARCH=loongarch64 fmt-check` 与 `git diff --check` 收口时复核。

用户区间仍由 MemorySet 检查；本轮不覆盖非法解除映射、OOM 注入、文件 mmap 或 mprotect。

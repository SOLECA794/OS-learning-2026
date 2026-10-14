# M3 lseek 边界

- 分类：RFC；日期：2026-10-14。
- 目标：RISC-V `lseek(62, fd, offset, whence)` 为 ramfs 普通文件和目录移动打开描述偏移，`dup`/`fork` 共享该偏移。
- 非目标：稀疏文件存储优化、文件删除/重命名、真实磁盘、LoongArch 用户态、`SEEK_DATA`/`SEEK_HOLE`。
- Owner：syscall 层拥有 Linux 编号、whence 与 errno 转换；VFS 打开描述拥有偏移；ramfs inode 拥有长度。
- Handoff：syscall 对 fd 表克隆描述后交给 VFS；不向 syscall 暴露偏移锁或 inode。
- Failure：坏 fd `EBADF`；控制台 `ESPIPE`；非法 whence/负目标 `EINVAL`；超出 `off_t` 上界 `EOVERFLOW`。失败不得改变偏移。
- Cleanup：无新长期资源；FD 生命周期保持现状。
- 受保护：现有 read/write/getdents 的偏移与 `dup`/`fork` 共享语义、Linux RISC-V 六参数分发框架、用户 ABI 布局。
- 验收：用户态断言 SEEK_SET/CUR/END、越过 EOF 后短读、目录重读、共享偏移、错误路径；RISC-V QEMU `test-vfs`、双架构编译、格式与 diff 检查。
- 停止：需要改变 owner、偏移 cookie 编码、现有读写 ABI 或无法保持失败原子性时停止并报告。

# M3 fstat 边界

- 分类：RFC；日期：2026-10-14。
- 目标：RISC-V `fstat(80, fd, statbuf)` 按 asm-generic 64 位 `struct stat` 的 128 字节布局返回已打开 ramfs 文件/目录及控制台的基础元数据。
- 非目标：`newfstatat`、真实 uid/gid/权限/时间戳、设备号管理、持久文件系统、LoongArch 用户态。
- Owner：ramfs inode 持有编号/类型/大小；VFS 打开描述只交出元数据快照；syscall 层独占 Linux UAPI 编码、fd 类型与用户指针。
- Handoff：syscall 克隆 fd 表项后释放 task 锁，取得 VFS 快照并编码 128 字节，再经现有用户写回通道复制。
- Failure：坏 fd `EBADF`，非法用户缓冲 `EFAULT`；不更改文件内容、打开偏移或 fd 表。
- Cleanup：快照和编码缓冲只在栈上，无新长期资源。
- 受保护：现有 VFS inode identity、read/write/lseek/getdents 偏移语义、现有 syscall ABI。
- 验收：用户态核对结构偏移/长度、文件与目录类型/大小、唯一 inode、控制台类型、坏 fd、坏指针与 4 字节 canary；QEMU 回归、双架构构建。
- 停止：若需要伪造权限/时间语义为真实值，或需要改变 inode owner、ABI 布局，停止上报。

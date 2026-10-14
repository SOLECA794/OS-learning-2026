# M3 pread64/pwrite64 边界

- 分类：RFC；日期：2026-10-14。
- 目标：RISC-V `pread64(67)` / `pwrite64(68)` 在 ramfs 普通文件指定非负位置读写，保持共享打开描述偏移不变。
- 非目标：vectored IO、direct IO、持久存储、超过当前单次一页上限的原子请求、LoongArch 用户态。
- Owner：syscall 负责参数、用户缓冲与 Linux 错误；VFS 打开描述负责访问模式与调用 inode；ramfs inode 负责内容。
- Handoff：fd 表克隆 `Arc<OpenFileDesc>` 后释放 task 锁；VFS 不接触用户指针，syscall 不接触 inode。
- Failure：坏 fd `EBADF`，控制台 `ESPIPE`，目录 `EISDIR`，负 offset `EINVAL`，无效用户缓冲 `EFAULT`；失败不改变文件或共享偏移。
- Cleanup：内核临时缓冲随 syscall 返回释放，无新长期资源。
- 受保护：read/write/lseek 的共享游标、dup/fork 共享描述、O_APPEND 现有行为、用户地址校验与单页有界 IO。
- 验收：QEMU 用户态验证指定位置读写、错误路径、指针失败不改内容/偏移、dup 共享偏移不动；双架构编译与现有回归。
- 停止：若需要修改 shared offset owner、read/write ABI 或引入静默语义降级，停止报告。

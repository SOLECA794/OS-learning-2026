# M3 mkdirat 目录 fd 边界

- 目标：`mkdirat(dirfd, path, mode)` 的相对路径可从目录 fd 逐段解析；绝对路径忽略 dirfd，`AT_FDCWD` 暂映射根目录。
- 所有权：syscall 层复制用户路径并解析 fd 表；VFS 层校验目录类型、查找父目录并原子创建 ramfs 节点。目录 fd 以 `Arc<OpenFileDesc>` 跨层传递，不外露 inode。
- 错误：坏 fd 为 `EBADF`，普通文件或控制台 fd 为 `ENOTDIR`；现有路径、重复创建及父目录错误保持不变。
- 不在本次：真实 cwd、权限/umask、`.`/`..`、符号链接、LoongArch 用户态运行。
- 验证：RISC-V QEMU 的 VFS 回归覆盖相对创建、绝对路径忽略 fd、错误 fd 和非目录 fd；LoongArch 编译检查。

# M3 `openat` 四参数 ABI 边界

- 日期：2026-10-14；分类：RFC，单次 ABI cutover。
- 目标：RISC-V 系统调用 56 按 `(dirfd, path, flags, mode)` 解析；绝对路径忽略 dirfd，相对路径从 AT_FDCWD 或打开的目录 fd 解析。仓库内用户程序一次迁移。
- 非目标：通用 cwd/chdir、挂载、符号链接、权限/umask、O_CLOEXEC、完整 Linux O_* 支持、LoongArch 用户态。
- Owner：`kernel/syscall/fd` 解释 Linux 参数及 fd 错误；`fs` 持有目录节点与逐段解析；`task` 仍只持有 fd 槽位。
- Handoff：syscall 复制用户路径、解析 flags/dirfd，向 VFS 传内核路径、领域选项和克隆的目录打开描述。
- Failure：无效用户指针 EFAULT；相对路径非法 dirfd EBADF、非目录 dirfd ENOTDIR；未知 flags EINVAL；检查不应创建文件或消耗 fd。
- Cleanup：成功生成的打开描述由 task FD 表接管；失败的临时 Arc 自动析构。
- 受保护契约：系统调用号 56、现有文件内容/偏移共享、VFS open/create/excl/trunc/append 语义、绝对路径与根名字结果；既有 mkdirat ABI 不变。
- 明确替换：旧私有 `(path, flags, mode)` 去除，仓库调用方全迁移。AT_FDCWD 暂指向固定根目录，cwd 模型落地时更换为进程 cwd。
- 验收：原有 VFS/进程/shell/COW/console 回归；目录 fd 相对多段路径、绝对路径忽略坏 dirfd、AT_FDCWD、EBADF/ENOTDIR/EFAULT/flags 错误定向测例。
- 验证方式：比赛容器 make 入口有界 QEMU，双架构构建。
- 停止条件：需修改 FD owner、文件树多后端抽象、外部受支持旧 ABI 调用方，或路径行为与已接受边界不一致。

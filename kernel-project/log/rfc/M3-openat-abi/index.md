# M3 `openat` 四参数切换

- 状态：Closed（四参数调用方式和目录 fd 相对路径定向回归通过）
- 日期：2026-10-14
- 作者：Codex；评审：用户的继续推进指令覆盖此单次窄范围切换。

## 目标与背景

Linux RISC-V `openat(56)` 参数依次是 `dirfd, path, flags, mode`。当前项目的 56 号系统调用是私有 `(path, flags, mode)`，仓库内 shell、主用户程序、VFS/COW 测试均按此调用。该桥在 M2.5 特意保留，并明确路由到 M3 统一迁移。

## 设计

- 语义边界：`../../boundary/M3-openat-abi.md`。
- Replace：syscall 分发改为四参数；同步仓库内所有调用方，不做旧 ABI 猜测分流。
- VFS 从目录打开描述中的 inode 取得相对解析起点；不向 syscall 泄漏 inode 类型或目录锁。
- 绝对路径不查 dirfd。AT_FDCWD 临时映射为现有根目录，因为当前没有进程 cwd；引入 cwd 后移除此固定根语义。
- 相对路径的 dirfd 要存在且必须为目录；有效目录允许多段路径，仍拒绝 `.`/`..` 与超长路径。
- Linux mode 暂未接权限模型，沿用 `mkdirat` 的显式限制；flags 只支持当前已实现集合。

## 实施与验收

先补调用方测试，验证旧 ABI 无法通过；完成单次切换后运行 `make ARCH=riscv64 test-vfs`、`test-process`、`test-shell`、`run-cow`、`test-console`，以及双架构构建。只有验收后更新 README 的当前契约。

## 收口记录

- 修复前：只迁移 `uapp-vfs` 后运行 `make ARCH=riscv64 test-vfs RUN_ID=20261014-openat-before`，串口输出 `VFS_FAIL:root flags`，有界运行器 60 秒后 `EXIT_TIMEOUT`。旧内核将 AT_FDCWD 误认作路径指针。
- 修复后：`make ARCH=riscv64 test-vfs RUN_ID=20261014-openat-after` PASS；串口输出 `M3_OPENAT_PASS`、`VFS_PASS`。定向测例覆盖目录 fd 的相对多段路径和创建/重开、AT_FDCWD、绝对路径忽略非法 dirfd、相对路径 EBADF/ENOTDIR、旧三参数调用返回 EFAULT。
- 回归：`make ARCH=riscv64 test-process/test-shell/run-cow/test-console RUN_ID=20261014-openat` 分别 PASS；COW 128 轮回收帧数稳定。`make fmt-check`、`git diff --check` 和普通 RISC-V `make build` PASS。证据在 `artifacts/runs/` 的对应目录。
- 双架构构建：LoongArch 在比赛容器临时副本依次 `make clean`、`make ARCH=loongarch64 defconfig`、`make ARCH=loongarch64 build` PASS，入口 `0x200000`；没有 LoongArch 用户态运行结论。
- 契约切换：系统调用号 56 不变，参数位置由私有三参数替换为四参数；无旧 ABI 兼容分流。路径解析由 VFS 独占 inode，syscall 仅向 VFS 交接克隆的目录描述；FD 表 owner 不变。
- 摩擦扫描：未增加第二份文件/目录状态，目录 inode 仅在 VFS 内部短暂克隆；绝对路径不查 dirfd；检查错误路径在创建文件和分配 fd 之前返回。固定根 cwd 是明确的阶段限制，进程 cwd 落地时移除。相对路径多段解析同时允许 VFS 内部 mkdir/exec 以根为起点使用这类路径，不宣称这些接口已完成 Linux dirfd 语义。
- 剩余：mode/umask、O_CLOEXEC 等 flags、`.`/`..`、cwd/chdir、符号链接与挂载仍待后续 M3；`mkdirat` 的相对目录 fd 尚不支持。

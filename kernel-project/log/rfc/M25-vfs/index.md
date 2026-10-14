# RFC：M2.5 最小 VFS 与分层 ramfs

## 状态

- 状态：Closed（最小 VFS 目标已通过运行验收；Linux 全量兼容仍属后续阶段）
- 日期：2026-10-14
- 作者：Codex
- 评审：项目维护者

## 1. 目标（Target）

- 在不退化现有文件 syscall 和 shell/exec 的前提下，引入稳定的目录/文件节点身份、统一打开描述与绝对路径解析；以 ramfs 为第一个后端，支持至少一层子目录。
- 非目标：ext4、挂载、相对路径/cwd、symlink、权限、设备节点、通用 trait 层级的预先扩张。
- 前置：现有 FD 表、ramfs、shell 与 QEMU 回归入口。

## 2. 动机与背景

- 变更前：`src/kernel/fs/ramfs.rs` 用 `Vec<RamFile>` 按去除前导斜杠后的字节串精确匹配，`OpenFileDesc` 保存文件名；`getdents64` 只能枚举扁平根目录，`execve` 直接读取 ramfs。
- 依据：`docs/08-development-process.md` 将 VFS/FD、ramfs、控制台、小 shell 列为 M2.5 文件线；`docs/01-architecture.md` 指定 `src/fs/` 为文件系统 owner。

## 3. 边界与所有权

- Owner：`src/fs/` 持有节点树、路径解析和 ramfs；`kernel::task` 只持 FD 表；`kernel::syscall` 只持 ABI/用户内存边界。
- Handoff：syscall 提交路径和标志，VFS 返回打开对象；读写与目录枚举经该对象，dup/fork 共享对象。
- Failure/Cleanup：创建与截断在 VFS 锁内完成；打开描述最后一个引用负责释放游标/偏移；失败的 exec 不替换地址空间。
- 受保护契约：现有根目录路径、共享偏移、脚本 shell、文件错误码和用户指针安全。
- 不变量：节点身份不依赖可变路径名；内容唯一保存在 ramfs；打开偏移唯一保存在共享打开描述。

## 4. 变更设计

- 实现：`src/fs/ramfs.rs::Inode` 的 `NodeData::{File, Directory}` 持有内容/子节点，目录用小 `Vec`；打开描述持 Arc 节点句柄、偏移锁与访问模式。绝对路径支持子目录；无前导斜杠的单段根文件名保留兼容，重复斜杠合并，空路径/超长路径拒绝。
- ABI 编码边界：syscall 将 Linux O_* 转为 `OpenOptions`，并将 VFS 的 `DirectoryEntry` 编码成 linux_dirent64；VFS 不持有 Linux 标志位或结构体布局。目录描述仅在调用方接受记录后推进共享游标。
- 不采用“只把 `ramfs` 改名为 `vfs`”：没有目录/节点身份，不能满足目标。
- 不提前引入多层 `FileSystemOps/SuperBlockOps/InodeOps/FileOps` trait：当前只有一个后端，第二个后端出现时再确定需要的最窄接口。
- ABI：Introduce `mkdirat=34(dirfd, path, mode)`（绝对路径或 AT_FDCWD 根文件名，mode 暂不实施权限检查）；Refine 路径语义、O_DIRECTORY/O_EXCL、O_APPEND 的真实追加行为和非目录 getdents64 的 ENOTDIR；内部 `FdEntry`/`OpenFileDesc` 结构 Replace。现有 openat=56 实际为私有三参数 `(path, flags, mode)`，本轮保留以避免破坏全部已有用户程序；迁移到 Linux 四参数 ABI 留待兼容线统一完成，不宣称 Linux ABI 全量兼容。
- 过渡：以一次最终切换让 syscall/exec 均走新 owner；切换前旧路径仍有效，不能存在两份内容真相。

## 5. 实施路线

- Stage 1：建立 VFS 节点和路径解析，保持现有 syscall 调用旧实现；用内核单元/编译检查验证节点模型。
- Stage 2：一次性切换 FD 与 exec 到新 owner，并添加子目录用户态用例；运行 shell/COW/console 回归及双架构编译。
- 所有构建走 `make`，QEMU 使用仓库有界运行器，保存 `result.json` 与串口日志。

## 6. 验收与停止条件

- 验收：见 `log/boundary/M25-vfs.md`；须有子目录读写、枚举、exec 与旧根目录回归证据。
- 停止：评审未接受；需要改变未声明的 Linux ABI、引入第三方 crate 或把 M2.5 扩成 ext4/挂载。
- 回滚：cutover 前删除未接线的新节点实现；cutover 后若验证失败，恢复旧 owner 的 syscall/exec 路径，保留日志供诊断。

## 7. 开放问题与已接受限制

- 第一版包含 `mkdirat`，定向测试从用户态创建目录后验证路径/枚举/exec，不依赖预置目录。路径重复斜杠按分隔符处理，末尾斜杠要求目录；`.`/`..` 暂返回 EINVAL，无 cwd/dirfd 相对路径解析。
- 已接受限制：单 ramfs 后端；无 cwd/dirfd 相对路径、挂载、删除/重命名、`.`/`..` 枚举项及权限。沿用项目 isize errno 约定；普通文件访问模式错误仍为 EACCES。没有第三方依赖。Arc 分配/exec 文件快照仍受现有内核 OOM 策略约束，不宣称资源耗尽测试已完成。

## 评审结论

- Review 1：用户在本草案交付后要求“继续推进进度”（2026-10-14），据此继续该 VFS 目标；目录创建选择 mkdirat，并在实现前说明现有 openat 私有 ABI 的保留边界。

## 实现与验证结果（agent-run，2026-10-14）

- 接线：旧 `kernel/fs` 被 `src/fs` 与 `kernel/task/fd_table.rs` 替代；旧程序源只用于 boot 填充，运行时 open/exec 共用一棵树。没有双份内容权威或路径名重新查询的打开描述。
- 修复同路径问题：另一描述截断后旧偏移超出 EOF 返回 0；O_APPEND 在内容锁内选择 EOF；目录记录包含 NUL 并按 8 字节对齐；文件 read/getdents64 在消费偏移前准备用户目的页。
- 测试：V1 目录/路径错误；V2 独立 open、dup、截断及跨描述追加；V3 目录类型错误、用户区未映射地址 EFAULT 且偏移不变；V4 目录记录、稳定 inode、共享游标、短缓冲与 EFAULT 不消费；V5 fork 共享偏移；V6 用户复制 ELF 到子目录后 exec，目标程序验证 argv，父进程验证退出码。

环境：`zhouzhouyi/os-contest:20260510`，主工作区 RISC-V，运行均由仓库有界运行器完成。

| 命令（容器内） | 结果 | 证据目录 / 标记 |
|---|---|---|
| `make ARCH=riscv64 test-vfs RUN_ID=20261014-vfs-boundary` | PASS | `artifacts/runs/vfs-20261014-vfs-boundary/`，V1–V6、VFS_EXEC_ARGS_PASS、VFS_PASS |
| `make ARCH=riscv64 test-process RUN_ID=20261014-vfs-boundary` | PASS | `artifacts/runs/process-20261014-vfs-boundary/`，G4/G5 完成后 task 0 exec 目标退出 7 |
| `make ARCH=riscv64 test-shell RUN_ID=20261014-vfs-boundary` | PASS | `artifacts/runs/shell-20261014-vfs-boundary/`，shell: done |
| `make ARCH=riscv64 run-cow RUN_ID=20261014-vfs-boundary-cow` | PASS | `artifacts/runs/20261014-vfs-boundary-cow/`，G6-COW-PASS |
| `make ARCH=riscv64 test-console RUN_ID=20261014-vfs-boundary` | PASS | `artifacts/runs/console-20261014-vfs-boundary/`，INPUT_PASS |
| `make fmt-check` | PASS | 容器退出 0 |
| `make ARCH=riscv64 build` | PASS | 最后恢复普通内核 ELF，入口 0x80200000 |
| 隔离副本 `make clean` → `make ARCH=loongarch64 defconfig` → `make ARCH=loongarch64 build` | PASS | 容器退出 0，LoongArch ELF 入口 0x200000；无 LA 用户态运行声明 |

LoongArch 在只读挂载源码的容器临时副本中单独 clean/defconfig/build；运行边界仍为此前 M1，未声称 LA 用户态 VFS 已运行。

参考：[Linux 打开标志定义](https://github.com/torvalds/linux/blob/master/include/uapi/asm-generic/fcntl.h)、[linux_dirent64 布局](https://github.com/torvalds/linux/blob/master/include/linux/dirent.h)。mkdirat 编号同时核对本机 `/usr/include/asm-generic/unistd.h`（34）。

收口：最小 VFS 目标完成，RISC-V M2.5 基础文件线已运行验证。下一步路由到 M2.8 的 wait4 失败原子性与回收；openat 标准 ABI/权限/挂载等兼容工作进入 M3。

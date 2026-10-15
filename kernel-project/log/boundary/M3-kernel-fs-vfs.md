# M3：文件系统归入 kernel 与多后端 VFS 边界

## 基本信息

- 任务：规划 `src/fs` 迁入 `src/kernel/fs`，建立第二个文件系统可接入的 VFS 边界。
- 分类：RFC（改变 owner、内部共享接口与文件节点表示）。
- 日期：2026-10-15。
- 状态：已完成本边界内的双架构分层运行验收；LoongArch 用户态文件 ABI 不在本轮目标内。

## 边界

- **目标**：文件系统的内核语义统一由 `kernel::fs` 拥有；`kernel::fs::vfs` 持有路径解析、打开描述和挂载解析，ramfs 只是一个后端。用一个最小的第二后端测试证明 VFS 不依赖 ramfs 的具体 `Inode` 类型。
- **非目标**：本轮不实现 ext4、块设备、持久化、页缓存、权限、符号链接、进程 cwd、完整 Linux 文件系统 ABI，也不宣称 LoongArch 用户态文件功能已运行。
- **Owner**：`kernel::fs::vfs` 拥有路径、打开对象和挂载边界；`kernel::fs::ramfs` 拥有文件树及内容；`kernel::task` 拥有 FD 槽位；`kernel::syscall` 拥有 Linux ABI 与用户指针边界。
- **Handoff**：syscall 把已复制到内核的路径和已解码选项交给 VFS；VFS 返回 `Arc<OpenFileDesc>`，FD 表只持句柄；VFS 经窄节点接口调用具体文件系统。
- **Failure**：路径解析、创建、截断、挂载失败不发布半成品；切换前后均保留单一根文件树真相。验证未通过则不更新当前架构文档，不宣称 cutover 完成。
- **Cleanup**：FD 槽位由 task 关闭；打开描述由最后一个 `Arc` 释放；节点及挂载引用由 VFS/后端释放，不能形成强引用环。

## 受保护契约

- **public API**：用户态系统调用号及参数次序不变；`openat`、`mkdirat`、`read/write`、`lseek`、`pread64/pwrite64`、`fstat`、`getdents64`、`execve` 的现有可见行为不退化。
- **ABI / 可见语义**：`dup`/`fork` 共享打开偏移；失败的用户内存访问不消费游标；`O_APPEND` 在文件内容锁内选择 EOF；目录 fd 相对路径继续可用；旧启动文件和 shell 测试继续可访问。
- **当前契约**：M2.5 RFC 中的 `src/fs` owner 在 cutover 前继续有效。本提案只改变内部模块归属，不放宽已验收的文件语义。
- **正确性不变量**：文件内容只存在于后端节点；路径不是节点身份；一个打开描述只持一个共享偏移；VFS 不保存第二份文件内容或目录树。

## 验收与验证

- **验收标准**：`src/kernel/fs` 成为唯一文件系统入口；不存在运行时 `crate::fs` 引用；VFS 不引用 `ramfs::Inode`；第二个最小后端能经同一 VFS 路径打开和读取，同时原 ramfs 用例不退化。
- **验证方式**：按项目 `make` 入口进行 `fmt-check`、双架构 `build`、RISC-V `test-vfs`、`test-process`、`test-shell`、`test-console`；QEMU 运行需另获明确授权，静态检查不冒充运行验证。
- **验证者**：agent-run（2026-10-15；RISC-V 用户态文件线与 LoongArch 内核级双后端自检，证据见 RFC）。
- **停止条件**：需要改变用户 ABI、削弱既有行为、引入第三方文件系统 crate、第二后端接口要求增加未经评审的生命周期/挂载语义，或验证环境不足以支持所声明的验收。

## 路线

1. 接受 RFC 后，将 VFS 与 ramfs 的共享节点接口定为最窄内部契约，补充独立后端测试。
2. 一次性切换 `main`、task、syscall、exec 的模块路径；旧 `src/fs` 不留转发桥或双份根目录。
3. 完成构建与被授权的运行回归后，再更新 `AGENTS.md`、架构文档、文件系统文档和 README 的当前目录说明。

设计与评审问题见 `log/rfc/M3-kernel-fs-vfs/index.md`。

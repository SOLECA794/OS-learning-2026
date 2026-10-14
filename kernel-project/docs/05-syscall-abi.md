# 05 · 系统调用与 ABI 兼容设计

## 1. 目标

系统调用层是评分最重的模块（一等奖自研代码 40-55% 集中于此）。目标：以 LTP 等测例分数为驱动的 Linux UAPI 兼容面，兼顾性能（syscall 快路径）。

## 2. 组成

```
src/kernel/syscall/
├── num.rs           syscall 编号表（对齐目标内核 ABI）
├── syscall.rs       分发入口：syscall(num, args) → SysResult<usize>
├── fs.rs / mm.rs / task.rs / ipc.rs / event.rs / futex.rs / time.rs / misc.rs / uid.rs
└── (辅助) uptr.rs / ubuf.rs   用户指针安全访问
```

## 3. ABI 边界规则（核心原则）

1. **Linux ABI 只出现在 syscall 边界**：结构体布局、flag、magic 常量在边界解析为内部领域类型；进入核心子系统后禁止出现 Linux 形状。
2. **errno 诚实性**：区分"不支持的功能"（ENOSYS/ENOTSUP）与"非法输入/权限失败"（EINVAL/EPERM/EBADF）；返回错误码错误会直接破坏测例。
3. **flag 校验**：未知 flag 的组合、矛盾组合、边界值（零大小、最大大小、溢出、未对齐地址、空指针）必须对齐 Linux 行为。
4. **静默兼容纪律**：难以实现的 flag 可以静默兼容（用户观测效果不变），但必须：注释说明 ABI 取舍与移除条件 + 打日志保持可观测性。
5. **用户指针安全**：所有用户数据经 uptr/ubuf 校验后访问，禁止直接解引用裸指针；copy-in/out 顺序与边界在 syscall 层统一。

当前 RISC-V `wait4(260)` 已按 4 字节 `int` 写回退出状态；失败写回不消费僵尸。
范围仅覆盖已有的退出码、指定子进程/任意子进程与 WNOHANG 子集；其余 Linux
`wait4` 语义不由这个字段宽度切换推断。验收见 `../log/rfc/M3-wait4-status/index.md`。
RISC-V `openat(56)` 已切换为 `(dirfd, path, flags, mode)`；绝对路径忽略 dirfd，
相对路径接受 AT_FDCWD 或目录 fd。当前仅有 ramfs、固定根 cwd，mode 尚不参与
权限检查，O_* 仅支持已实现子集。验收见 `../log/rfc/M3-openat-abi/index.md`。
RISC-V `lseek(62)` 已支持 ramfs 普通文件与目录的 `SEEK_SET/CUR/END`，
`dup`/`fork` 共享打开描述偏移；控制台不可 seek。验收见 `../log/rfc/M3-lseek/index.md`。
RISC-V `pread64(67)` / `pwrite64(68)` 已支持 ramfs 普通文件定位 IO，不改变共享偏移；
`O_APPEND` 下定位写仍追加。验收见 `../log/rfc/M3-positioned-io/index.md`。
RISC-V `fstat(80)` 已按 64 位 asm-generic `struct stat` 布局写回 128 字节，
当前可靠字段仅包括节点编号、类型、普通文件长度与逻辑块数；
uid/gid、权限位和时间戳暂无模型。验收见 `../log/rfc/M3-fstat/index.md`。

## 4. 实现优先级（由测例分数驱动）

优先序不是"感觉重要"，而是 **LTP 子测例计数**（TPASS 算一分）：

1. **进程族**：fork/execve/exit/wait4/clone/getpid/ppid/setsid 等（子测例极多）。
2. **文件系统**：open/read/write/close/lseek/stat/fstat/access/readdir 等（同样高密度）。
3. **信号**：sigaction/sigprocmask/kill/rt_sigreturn/rt_sigpending。
4. **内存**：mmap/munmap/mprotect/brk/mremap（含 flag 矩阵）。
5. **IPC/同步**：pipe/futex/eventfd（epoll 后置）。
6. **网络**：socket/bind/listen/accept/connect/read/write + TCP loopback。
7. **时钟**：clock_gettime/gettimeofday/nanosleep/timer_create。

每个 syscall 落地模板（供开发团队使用）：

```
1. 阅读 LTP 对应测例源码，确认期望语义（不要凭名字猜）。
2. 实现边界解析（结构体/flag/errno）→ 内部语义 → 返回。
3. 补用户态测试（窄范围、输出可 grep、失败路径明确）。
4. 双架构编译 + 定向测例 + 回归集。
5. 记录 ABI 取舍（若静默兼容：注释 + 日志 + 移除条件）。
```

## 5. 性能关注点（syscall 快路径）

- trap 入口/返回热路径：减少不必要保存面（一等奖普遍在此做四级缓存：current_task/process_identity/return_work/trap_context）。
- 参数解析避免重复拷贝；用户指针校验零成本化（范围预校验）。
- 高频 syscall（read/write/gettimeofday）单独快路径。
- vDSO 覆盖 clock_gettime/gettimeofday（减少陷入次数）。

## 6. 验收标准

1. 每个已实现 syscall：有 LTP 对应测例通过记录（分数台账）。
2. errno 映射抽查：非法参数返回与 Linux 一致（用对比脚本）。
3. 用户指针安全：越界/空指针不 panic（返回 EFAULT）。
4. syscall 延迟基准：getpid 延迟纳入台账（一等奖测 syscall 延迟上界）。
5. 双架构编号表一致（RV 与 LA 的 syscall 编号对齐各自 ABI 或统一映射，以目标 ABI 为准）。

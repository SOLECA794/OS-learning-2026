# G6：用户内存读取改为受控拷贝

- 分类：小迭代；日期：2026-10-14。
- 目标：`write`、`openat`、`execve` 读取用户数据时，逐页校验用户可读映射，并复制到内核拥有的缓冲区；非法映射返回 EFAULT，不产生内核缺页停机。
- 非目标：改变 syscall 编号/返回约定、修复 wait4 回收或 read 共享偏移提交顺序、支持 LoongArch 用户态、用户页 demand paging。
- Owner：`MemorySet` 拥有 PTE 翻译与权限判定；syscall 拥有 ABI 地址边界和 EFAULT 映射。
- Handoff：syscall 传地址与长度，MemorySet 返回内核 `Vec<u8>`；调用者不持有指向用户页的 Rust 引用。
- Failure：先校验全部页，再分配与复制；无效地址 EFAULT。长度受现有调用者约束，`write` 一次最多一页以防止用户长度导致巨大堆分配。
- Cleanup：返回的 Vec 由调用者释放；MemorySet 不接管用户页所有权。
- 受保护契约：syscall ABI 不变；合法输入的数据和返回值不变（大 `write` 允许短写）；COW 只读共享页应仍可读。
- 验收：定向用户程序对未映射地址执行 `write` 返回 EFAULT，且随后仍能执行；双架构编译通过。
- 验证：比赛容器内 `make build-cow`、`make build`、`make fmt-check`；QEMU 运行验收需用户明确要求。
- 停止条件：若需要改动 task/fs 的状态提交、公共 ABI、或引入跨核并发语义，则停止上报。
- 路线：单 checkpoint 完成实现和编译验证；运行验证单列待办。

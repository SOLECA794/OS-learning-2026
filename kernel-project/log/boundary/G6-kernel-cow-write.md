# G6：内核向 COW 用户页写回

- 分类：小迭代；日期：2026-10-14。
- 目标：`wait4` 的状态写回与文件读入用户缓冲区，在 COW 页上先解除共享再写入；无效或只读映射返回 EFAULT。
- 非目标：重做用户读指针、LoongArch 用户态、SMP、文件系统语义。
- Owner：`MemorySet` 拥有页表与 COW 状态；syscall 只传用户地址和字节切片。
- Handoff：syscall 经 `task::with_current_space` 调用 `MemorySet::copy_to_user`；失败映射为 EFAULT。
- Failure：先校验整个目的区间，再解除 COW，最后复制；失败时不写入目的字节。COW 解除过程中若分配失败，已解除页仍归当前地址空间所有。
- Cleanup：新页与旧页引用均由现有 `handle_cow_store`/`MemorySet` 负责。
- 受保护契约：Linux syscall 编号/参数/返回编码不变；父子 COW 隔离不变；不扩大 public API。
- 验收：`uapp-cow` 增加非空 `wait4(wstatus)`，状态为 `5 << 8`；双架构构建通过。
- 验证：agent 执行 `make build-cow` 与 LoongArch `make build`；运行验收需用户执行 `make run-cow`（仓库规则禁止 agent 自行启动 QEMU）。
- 停止条件：若需修改 ABI、owner、或无法在现有地址空间接口内维持上述失败语义，则停止上报。
- 路线：单 checkpoint 完成实现和编译验证；运行验证单列待办。

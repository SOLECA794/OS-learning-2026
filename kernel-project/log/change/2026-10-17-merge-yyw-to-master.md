# 2026-10-17 — 合并 origin/yyw 进 master（aa6bb46）

## 范围
将队友 yyw 分支（92bcd1c）合并进 master；本地 VirtIO 调试提交（eee3638）与文档（0be42d6）一并落地。

## 合入内容

### LoongArch64 对齐到 M2.8（test-la-* 10 项 PASS）
- M1.5 user-trap、M2 process/exec/上下文切换、M2.5 console RX 阻塞唤醒/文件 FD/dir/file ops/pipe。
- 单核用户态时钟抢占（ECFG.LIE/TICLR）。
- brk/mmap/munmap 用户态接入。
- M2.8 COW OOM：134 故障点、128 轮临时地址空间回收、128 次真实用户 fork/COW/exit/wait4。
- 未完成：IPI/多核、信号、文件 mmap/mprotect、定时器频率校准、实板。

### 多后端 VFS 抽象
- `src/fs` → `src/kernel/fs`；对象安全 `VfsNode` trait 切断对具体 ramfs 类型的依赖。
- 第二后端 staticfs（固定只读静态树，挂载 `/static`）验证同一路径解析，非 ext4/持久化。

### RV 侧修正（队友）
- `munmap(addr,len)` 原忽略 len、只删恰好以 addr 起始的整区；现按页范围解除、拆分左右保留区、释放 COW/独占引用并刷 TLB。
- `mmap_anon` 原把向上取整页数误当字节数，多页只映射一页；现按页大小还原并校验零长度/溢出。
- 修正 brk/mmap/munmap/pipe 只读端重复取反 errno。

## 冲突处理
- 仅 `src/drivers/virtio_blk.rs` 冲突：保留本地队列大小 min(qnum_max) 与 QUEUE_ALIGN，删除与公共段重复的 pfn/容量/DRIVER_OK。队友该文件仅 rustfmt，无逻辑冲突。

## 验证（agent-run）
- RV64：test / test-vfs / test-shell / test-process / test-brk / test-pipe / test-mmap / test-console / test-fault / run-cow(128 轮) / test-cow-oom(520 点) 全 PASS。
- LA64：test-la-process/exec/file/fd-ops/dir/pipe/preempt/memory/cow-oom/console 全 PASS；主工作区 LA 内核构建通过。

## 边界
- VirtIO-Blk 仍处探测调试：devid=0、写 Status 不生效，磁盘保持禁用，未影响回归。
- 当前单核；SMP 数据结构仅预留。

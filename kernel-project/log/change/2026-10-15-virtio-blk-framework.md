# 2026-10-15 — VirtIO-Blk 驱动框架

## 范围
P1 第一步：VirtIO-Blk MMIO 驱动框架搭建。

## 新增

- `src/drivers/mod.rs`：驱动层入口，VIRTIO_BLK 全局 SpinLock
- `src/drivers/virtio_blk.rs`：MMIO 探测 + 队列初始化 + 同步扇区读
- QEMU virt 平台加 VIRTIO_BLK MMIO 映射（0x1000_1000）
- 启动流程 fs::init() 后调 drivers::init()

## 当前状态
- 驱动框架编译通过
- QEMU 未挂磁盘镜像，设备不出现，init() 返回 false
- 下一步：创建磁盘镜像 + QEMU -drive 挂载 + 扇区读验证

## 后续
- 创建 ext4/FAT32 磁盘镜像
- 从磁盘加载 ELF 执行（脱离 include_bytes!）
- 异步中断队列（当前轮询）

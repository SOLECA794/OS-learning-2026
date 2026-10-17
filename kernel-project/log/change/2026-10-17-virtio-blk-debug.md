# 2026-10-17 — VirtIO-Blk MMIO 设备探测调试（进行中，未闭环）

## 范围
P1 VirtIO-Blk 第二步：QEMU 挂载 raw 磁盘镜像后探测设备，定位"读通写不通"卡点。

## 变更

- `build.mk`：QEMU 命令行加 `-drive file=disk.img,if=none,id=disk0,format=raw -device virtio-blk-device,drive=disk0`。
- `disk.img`：10MB 全零 raw 镜像，本地产物，已被 `.gitignore` 忽略，不入版本库。
- `src/drivers/virtio_blk.rs`：探测阶段打印 magic/version/devid/vendor/status/host_features/queue_num_max，便于定位。

## 已验证事实（agent-run，QEMU 8.2.2，`make ARCH=riscv64 test` 仍 PASS）

- 设备地址 `0x1000_1000` 正确：magic=`0x74726976`、version=`1`、vendor=`0x554d4551`("QEMU") 均读对。
- MMIO 读通路正常；UART0(`0x1000_0000`) 写日志正常，说明 MMIO 写通路本身未坏。
- 设备区域以 `MemoryType::Device` + R/W 权限映射。

## 卡点（未解决，禁止宣称设备就绪）

- `devid`(0x008) 读回 `0`，应为 `2`(block)。
- 写 Status（ACKNOWLEDGE/DRIVER）后读回仍为 `0`，写寄存器不生效。
- `host_features=0`、`queue_num_max=0`，设备未进入激活态。
- 已分别尝试 legacy（Status=0x060）与 modern（Status=0x070）两套偏移，写均不生效；不再重复试这两套。

## 下一步排查方向（未执行）

- 核对 QEMU 8.2 `virtio-blk-device` 在 virt 机器上的实际模式（legacy/modern）与寄存器行为，必要时显式强制 modern 模式参数。
- 对照 xv6-riscv 初始化序列查漏（host_features 选择、FEATURES_OK、queue ready 顺序）。
- 用 QEMU monitor / `-d` 日志确认寄存器访问是否落到正确设备实例。

## 边界

- 本提交只到探测阶段，未实现队列下发、扇区读、中断；`ready=false`，磁盘保持禁用，不影响既有回归。

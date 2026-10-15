//! 设备驱动层。

mod virtio_blk;

use crate::klib::sync::SpinLock;
use virtio_blk::VirtioBlk;

pub(crate) static VIRTIO_BLK: SpinLock<Option<VirtioBlk>> = SpinLock::new(None);

pub(crate) fn init() {
    let blk = VirtioBlk::new();
    if blk.init() {
        // 读第一个扇区验证
        let mut buf = [0u8; 512];
        match blk.read_sector(0, &mut buf) {
            Ok(()) => crate::log_info!("[virtio-blk] sector 0 read OK, first byte=0x{:02x}", buf[0]),
            Err(e) => crate::log_error!("[virtio-blk] sector 0 read failed: {}", e),
        }
        *VIRTIO_BLK.lock() = Some(blk);
    } else {
        crate::log_info!("[virtio-blk] not found, disk disabled");
    }
}

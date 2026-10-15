//! 设备驱动层。

mod virtio_blk;

use crate::klib::sync::SpinLock;
use virtio_blk::VirtioBlk;

pub(crate) static VIRTIO_BLK: SpinLock<Option<VirtioBlk>> = SpinLock::new(None);

pub(crate) fn init() {
    let blk = VirtioBlk::new();
    if blk.init() {
        *VIRTIO_BLK.lock() = Some(blk);
    } else {
        crate::log_info!("[virtio-blk] not found, disk disabled");
    }
}

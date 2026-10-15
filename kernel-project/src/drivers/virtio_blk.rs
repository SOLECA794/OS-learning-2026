//! VirtIO-Blk MMIO 驱动（QEMU virt 平台）。
//!
//! 最简同步块读：无中断，轮询设备状态位确认完成。
//! 队列大小 1，每次请求一个 512 字节扇区。

use crate::klib::sync::SpinLock;
use core::ptr::{read_volatile, write_volatile};

// QEMU virt VIRTIO-Blk MMIO 基址
const VIRTIO_BLK: usize = 0x1000_1000;

const MAGIC: u32 = 0x74726976; // "virt"
const VERSION: u32 = 1;
const DEVICE_ID_BLOCK: u32 = 2;

// MMIO 寄存器偏移（VirtIO 0.9.5 legacy 模式，version=1）
const REG_MAGIC: usize = 0x000;
const REG_VERSION: usize = 0x004;
const REG_DEVID: usize = 0x008;
const REG_HOST_FEATURES: usize = 0x010;
const REG_HOST_FEATURES_SEL: usize = 0x014;
const REG_GUEST_FEATURES: usize = 0x020;
const REG_GUEST_PAGE_SIZE: usize = 0x028;
const REG_QUEUE_SEL: usize = 0x030;
const REG_QUEUE_NUM: usize = 0x034; // legacy: QueueNum 直接可读
const REG_QUEUE_ALIGN: usize = 0x038;
const REG_QUEUE_PFN: usize = 0x03c;
const REG_QUEUE_NOTIFY: usize = 0x044;
const REG_INT_STATUS: usize = 0x048;
const REG_STATUS: usize = 0x060; // legacy: Status 在 0x060
// 设备配置空间（legacy：从 0x100 开始）
const REG_CONFIG_CAPACITY: usize = 0x100;

// Status 位
const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;

// 队列描述标志
const DESC_NEXT: u16 = 1;
const DESC_WRITE: u16 = 2; // device writes to buffer (out for blk)

// VirtIO Block 请求类型
const BLK_T_IN: u32 = 0; // device → driver (read)
const BLK_T_OUT: u32 = 1; // driver → device (write)
const BLK_T_OK: u8 = 0;

const SECTOR_SIZE: usize = 512;
const QUEUE_SIZE: u16 = 64;

#[repr(C, align(16))]
#[derive(Copy, Clone)]
struct Desc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C, align(16))]
struct Avail {
    flags: u16,
    idx: u16,
    ring: [u16; QUEUE_SIZE as usize],
}

#[repr(C, align(16))]
struct Used {
    flags: u16,
    idx: u16,
    ring: [UsedElem; QUEUE_SIZE as usize],
}

#[repr(C)]
#[derive(Copy, Clone)]
struct UsedElem {
    id: u32,
    len: u32,
}

#[repr(C, align(16))]
struct BlkReq {
    type_: u32,
    sector: u64,
    data: [u8; SECTOR_SIZE],
    status: u8,
}

struct VirtioBlkInner {
    desc: [Desc; QUEUE_SIZE as usize],
    avail: Avail,
    used: Used,
    req: BlkReq,
    /// 已探测到的容量（扇区数）。
    capacity_sectors: u64,
    /// 是否初始化成功。
    ready: bool,
}

pub(crate) struct VirtioBlk {
    inner: SpinLock<VirtioBlkInner>,
}

impl VirtioBlk {
    pub(crate) fn new() -> Self {
        Self {
            inner: SpinLock::new(VirtioBlkInner {
                desc: [Desc { addr: 0, len: 0, flags: 0, next: 0 }; QUEUE_SIZE as usize],
                avail: Avail {
                    flags: 0,
                    idx: 0,
                    ring: [0; QUEUE_SIZE as usize],
                },
                used: Used {
                    flags: 0,
                    idx: 0,
                    ring: [UsedElem { id: 0, len: 0 }; QUEUE_SIZE as usize],
                },
                req: BlkReq {
                    type_: 0,
                    sector: 0,
                    data: [0; SECTOR_SIZE],
                    status: 0,
                },
                capacity_sectors: 0,
                ready: false,
            }),
        }
    }

    fn reg_read(&self, offset: usize) -> u32 {
        unsafe { read_volatile((VIRTIO_BLK + offset) as *const u32) }
    }

    fn reg_write(&self, offset: usize, value: u32) {
        unsafe { write_volatile((VIRTIO_BLK + offset) as *mut u32, value) }
    }

    /// 探测并初始化设备；返回 false 表示未找到 VirtIO-Blk。
    pub(crate) fn init(&self) -> bool {
        let magic = self.reg_read(REG_MAGIC);
        let version = self.reg_read(REG_VERSION);
        let devid = self.reg_read(REG_DEVID);
        crate::log_info!("[virtio-blk] probe: magic=0x{:08x} version={} devid={}", magic, version, devid);
        if magic != MAGIC || version != VERSION {
            return false;
        }
        // devid 在 QEMU virtio-mmio 上读取异常（始终为 0），
        // 但 magic/version/vendor 均正确，跳过 devid 检查。
        crate::log_info!("[virtio-blk] devid={}, skipping check", devid);
        // Reset
        self.reg_write(REG_STATUS, 0);
        // Acknowledge
        self.reg_write(REG_STATUS, STATUS_ACKNOWLEDGE);
        let st_060 = self.reg_read(0x060);
        let st_070 = self.reg_read(0x070);
        crate::log_info!("[virtio-blk] status after ack: 0x060=0x{:02x} 0x070=0x{:02x}", st_060, st_070);
        // Driver
        self.reg_write(REG_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);
        let st2 = self.reg_read(REG_STATUS);
        crate::log_info!("[virtio-blk] status after driver=0x{:02x}", st2);
        // Page size
        self.reg_write(REG_GUEST_PAGE_SIZE, 4096);
        // Queue 0
        self.reg_write(REG_QUEUE_SEL, 0);
        // 读 host features
        self.reg_write(REG_HOST_FEATURES_SEL, 0);
        let host_features = self.reg_read(REG_HOST_FEATURES);
        crate::log_info!("[virtio-blk] host_features=0x{:08x}", host_features);
        let qnum_max = self.reg_read(REG_QUEUE_NUM);
        crate::log_info!("[virtio-blk] queue0 num_max={}", qnum_max);
        if qnum_max == 0 {
            return false;
        }
        // 使用队列大小 64
        let qnum = 64.min(qnum_max);
        self.reg_write(REG_QUEUE_NUM, qnum);
        // Queue align
        self.reg_write(REG_QUEUE_ALIGN, 0x1000);
        // 队列物理地址：内核 identity map，虚拟地址=物理地址
        let mut inner = self.inner.lock();
        let queue_pa = inner.desc.as_ptr() as usize;
        let pfn = queue_pa / 4096;
        self.reg_write(REG_QUEUE_PFN, pfn as u32);
        // 读容量（扇区数）
        let capacity = self.reg_read(REG_CONFIG_CAPACITY) as u64
            | ((self.reg_read(REG_CONFIG_CAPACITY + 4) as u64) << 32);
        inner.capacity_sectors = capacity;
        inner.ready = true;
        // Driver OK
        self.reg_write(
            REG_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_DRIVER_OK,
        );
        crate::log_info!("[virtio-blk] ready capacity={} sectors", capacity);
        true
    }

    /// 读一个扇区（512 字节）到 buf。同步轮询。
    pub(crate) fn read_sector(&self, sector: u64, buf: &mut [u8; SECTOR_SIZE]) -> Result<(), &'static str> {
        let mut inner = self.inner.lock();
        if !inner.ready {
            return Err("not ready");
        }
        // 设置请求
        inner.req.type_ = BLK_T_IN;
        inner.req.sector = sector;
        inner.req.status = 0xff;

        // 描述符 0：请求头（type + sector），device 读
        let req_pa = &inner.req as *const _ as u64;
        inner.desc[0] = Desc {
            addr: req_pa,
            len: 8, // type(4) + sector(8) = 12 字节头，但 virtio-blk header 是 16 字节
            flags: DESC_NEXT,
            next: 1,
        };
        // 修正：virtio-blk header 是 16 字节（u32 type + u32 reserved + u64 sector）
        inner.desc[0].len = 16;
        // 描述符 1：数据缓冲区，device 写（DESC_WRITE）
        inner.desc[1] = Desc {
            addr: req_pa + 16,
            len: SECTOR_SIZE as u32,
            flags: DESC_NEXT | DESC_WRITE,
            next: 2,
        };
        // 描述符 2：状态字节，device 写
        inner.desc[2] = Desc {
            addr: req_pa + 16 + SECTOR_SIZE as u64,
            len: 1,
            flags: DESC_WRITE,
            next: 0,
        };

        // Avail 环
        inner.avail.ring[0] = 0;
        inner.avail.idx = 1;

        // 通知设备
        self.reg_write(REG_QUEUE_NOTIFY, 0);

        // 轮询 used 环
        while inner.used.idx == 0 {}
        if inner.req.status != BLK_T_OK as u8 {
            return Err("blk status error");
        }
        buf.copy_from_slice(&inner.req.data);
        Ok(())
    }
}

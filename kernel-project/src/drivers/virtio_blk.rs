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

// MMIO 寄存器偏移
const REG_MAGIC: usize = 0x000;
const REG_VERSION: usize = 0x004;
const REG_DEVID: usize = 0x008;
const REG_GUEST_PAGE_SIZE: usize = 0x010;
const REG_QUEUE_SEL: usize = 0x014;
const REG_QUEUE_NUM: usize = 0x018;
const REG_QUEUE_PFN: usize = 0x020;
const REG_QUEUE_NOTIFY: usize = 0x030;
const REG_INT_STATUS: usize = 0x044;
const REG_STATUS: usize = 0x070;
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
        if self.reg_read(REG_MAGIC) != MAGIC || self.reg_read(REG_VERSION) != VERSION {
            return false;
        }
        if self.reg_read(REG_DEVID) != DEVICE_ID_BLOCK {
            return false;
        }
        // Reset
        self.reg_write(REG_STATUS, 0);
        // Acknowledge
        self.reg_write(REG_STATUS, STATUS_ACKNOWLEDGE);
        // Driver
        self.reg_write(REG_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);
        // Page size
        self.reg_write(REG_GUEST_PAGE_SIZE, 4096);
        // Queue 0
        self.reg_write(REG_QUEUE_SEL, 0);
        if self.reg_read(REG_QUEUE_NUM) == 0 {
            return false;
        }
        self.reg_write(REG_QUEUE_NUM, QUEUE_SIZE as u32);
        // 队列物理页地址（我们的内核 identity map，虚拟=物理）
        let pfn = self.inner.lock().desc.as_ptr() as usize / 4096;
        self.reg_write(REG_QUEUE_PFN, pfn as u32);
        // 读容量
        let cap_lo = self.reg_read(REG_CONFIG_CAPACITY) as u64;
        let cap_hi = self.reg_read(REG_CONFIG_CAPACITY + 4) as u64;
        let capacity = (cap_hi << 32) | cap_lo;
        // Driver OK
        self.reg_write(REG_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_DRIVER_OK);

        let mut inner = self.inner.lock();
        inner.capacity_sectors = capacity;
        inner.ready = true;
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

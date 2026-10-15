//! 管道：内核环形缓冲区，读端和写端通过 Arc 共享。
//!
//! 容量固定 4096 字节；读空时 read 返回 0（EOF 语义暂不阻塞，先做非阻塞版本）。
//! 写满时 write 返回已写字节数（写不下的部分丢弃，后续扩展为阻塞）。

use crate::klib::sync::SpinLock;
use alloc::sync::Arc;

const PIPE_CAP: usize = 4096;

pub(crate) struct Pipe {
    buf: SpinLock<PipeBuf>,
}

struct PipeBuf {
    data: [u8; PIPE_CAP],
    read: usize,
    write: usize,
    /// 已写入但未读的字节数。
    len: usize,
    read_eof: bool,
}

impl Pipe {
    pub(crate) fn new() -> Arc<Pipe> {
        Arc::new(Self {
            buf: SpinLock::new(PipeBuf {
                data: [0; PIPE_CAP],
                read: 0,
                write: 0,
                len: 0,
                read_eof: false,
            }),
        })
    }

    /// 读端读取最多 buf.len() 字节；无数据返回 0。
    pub(crate) fn read(&self, buf: &mut [u8]) -> usize {
        let mut p = self.buf.lock();
        let n = p.len.min(buf.len());
        for i in 0..n {
            let r = p.read;
            buf[i] = p.data[r];
            p.read = (r + 1) % PIPE_CAP;
        }
        p.len -= n;
        n
    }

    /// 写端写入最多 buf.len() 字节；写满返回已写数。
    pub(crate) fn write(&self, buf: &[u8]) -> usize {
        let mut p = self.buf.lock();
        let space = PIPE_CAP - p.len;
        let n = space.min(buf.len());
        for i in 0..n {
            let w = p.write;
            p.data[w] = buf[i];
            p.write = (w + 1) % PIPE_CAP;
        }
        p.len += n;
        n
    }

    /// 读端关闭时标记 EOF（写端 read 时返回 0）。
    pub(crate) fn close_read(&self) {
        let mut p = self.buf.lock();
        p.read_eof = true;
    }
}

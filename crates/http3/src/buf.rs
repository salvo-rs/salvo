use std::collections::VecDeque;
use std::io::IoSlice;

use bytes::{Buf, Bytes};

pub(crate) struct BufList<T> {
    bufs: VecDeque<T>,
}

impl<T: Buf> BufList<T> {
    pub(crate) fn new() -> BufList<T> {
        BufList { bufs: VecDeque::new() }
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn push(&mut self, buf: T) {
        debug_assert!(buf.has_remaining());
        self.bufs.push_back(buf);
    }

    pub fn cursor(&self) -> Cursor<T> {
        Cursor {
            buf: self,
            pos_total: 0,
            index: 0,
            pos_front: 0,
        }
    }
}

impl BufList<Bytes> {
    pub fn take_chunk(&mut self, max_len: usize) -> Option<Bytes> {
        let chunk = self
            .bufs
            .front_mut()
            .map(|chunk| chunk.split_to(usize::min(max_len, chunk.remaining())));
        if let Some(front) = self.bufs.front() {
            if front.remaining() == 0 {
                let _ = self.bufs.pop_front();
            }
        }
        chunk
    }

    pub fn push_bytes<T>(&mut self, buf: &mut T)
    where
        T: Buf,
    {
        debug_assert!(buf.has_remaining());
        self.bufs.push_back(buf.copy_to_bytes(buf.remaining()))
    }
}

#[cfg(test)]
impl<T: Buf> From<T> for BufList<T> {
    fn from(b: T) -> Self {
        let mut buf = Self::new();
        buf.push(b);
        buf
    }
}

impl<T: Buf> Buf for BufList<T> {
    #[inline]
    fn remaining(&self) -> usize {
        self.bufs.iter().map(|buf| buf.remaining()).sum()
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        self.bufs.front().map(Buf::chunk).unwrap_or_default()
    }

    #[inline]
    fn advance(&mut self, mut cnt: usize) {
        while cnt > 0 {
            {
                let front = &mut self.bufs[0];
                let rem = front.remaining();
                if rem > cnt {
                    front.advance(cnt);
                    return;
                } else {
                    front.advance(rem);
                    cnt -= rem;
                }
            }
            self.bufs.pop_front();
        }
    }

    #[inline]
    fn chunks_vectored<'t>(&'t self, dst: &mut [IoSlice<'t>]) -> usize {
        if dst.is_empty() {
            return 0;
        }
        let mut vecs = 0;
        for buf in &self.bufs {
            vecs += buf.chunks_vectored(&mut dst[vecs..]);
            if vecs == dst.len() {
                break;
            }
        }
        vecs
    }
}

pub struct Cursor<'a, B> {
    buf: &'a BufList<B>,
    pos_total: usize, // position amongst all bytes
    pos_front: usize, // position in the current front buffer
    index: usize,     // current front buffer index
}

impl<'a, B: Buf> Cursor<'a, B> {
    pub fn position(&self) -> usize {
        self.pos_total
    }
}

impl<'a, B: Buf> Buf for Cursor<'a, B> {
    #[inline]
    fn remaining(&self) -> usize {
        self.buf.remaining() - self.pos_total
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        &self.buf.bufs[self.index].chunk()[self.pos_front..]
    }

    #[inline]
    fn advance(&mut self, mut cnt: usize) {
        assert!(cnt <= self.buf.remaining() - self.pos_total);
        while cnt > 0 {
            {
                let front = &self.buf.bufs[self.index];
                let rem = front.remaining() - self.pos_front;
                if rem > cnt {
                    self.pos_total += cnt;
                    self.pos_front += cnt;
                    return;
                } else {
                    self.pos_total += rem;
                    self.pos_front = 0;
                    cnt -= rem;
                }
            }
            self.index += 1;
        }
    }

    #[inline]
    fn chunks_vectored<'t>(&'t self, dst: &mut [IoSlice<'t>]) -> usize {
        self.buf.chunks_vectored(dst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn cursor_advance() {
        let buf = BufList::from(Bytes::from_static(&[1u8, 2, 3, 4]));
        let mut cur = buf.cursor();
        cur.advance(2);
        assert_eq!(cur.remaining(), 2);
        cur.advance(2);
        assert_eq!(cur.remaining(), 0);
    }
}

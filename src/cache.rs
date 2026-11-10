//! Small tables saved between runs, so that start-up does not rebuild them.
//!
//! A cache file is a version word followed by length-prefixed sections of
//! plain integers. It is only ever read by the build that wrote it or one
//! with the same version, on the same machine.

use crate::tables::table_dir;
use std::fs;
use std::io::Write;

/// Bump when the content of any cached table changes.
const VERSION: u64 = 8;

pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Writer {
        Writer { buf: VERSION.to_le_bytes().to_vec() }
    }

    /// Appends a slice of plain integers (or arrays of them).
    pub fn put<T: Copy>(&mut self, v: &[T]) {
        let bytes = std::mem::size_of_val(v);
        self.buf.extend_from_slice(&(bytes as u64).to_le_bytes());
        // T is a plain integer type or an array of one: every byte is initialised.
        self.buf.extend_from_slice(unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, bytes) });
    }

    pub fn save(self, name: &str) {
        let dir = table_dir();
        let _ = fs::create_dir_all(&dir);
        let tmp = dir.join(format!("{}.tmp", name));
        let ok = fs::File::create(&tmp).and_then(|mut f| f.write_all(&self.buf)).is_ok();
        if !ok || fs::rename(&tmp, dir.join(name)).is_err() {
            eprintln!("warning: could not cache {}", name);
        }
    }
}

impl Default for Writer {
    fn default() -> Writer {
        Writer::new()
    }
}

pub struct Reader {
    buf: Vec<u8>,
    pos: usize,
}

impl Reader {
    /// Opens a cache file written by `Writer` with the current version.
    pub fn open(name: &str) -> Option<Reader> {
        let buf = fs::read(table_dir().join(name)).ok()?;
        if buf.len() < 8 || buf[..8] != VERSION.to_le_bytes() {
            return None;
        }
        Some(Reader { buf, pos: 8 })
    }

    /// Reads the next section, which must hold exactly `len` items.
    pub fn get<T: Copy>(&mut self, len: usize) -> Option<Vec<T>> {
        let head = self.buf.get(self.pos..self.pos + 8)?;
        let bytes = u64::from_le_bytes(head.try_into().ok()?) as usize;
        if bytes != len * std::mem::size_of::<T>() {
            return None;
        }
        let data = self.buf.get(self.pos + 8..self.pos + 8 + bytes)?;
        let mut out: Vec<T> = Vec::with_capacity(len);
        // T is a plain integer type or an array of one, so any bytes are a valid value.
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), out.as_mut_ptr() as *mut u8, bytes);
            out.set_len(len);
        }
        self.pos += 8 + bytes;
        Some(out)
    }
}

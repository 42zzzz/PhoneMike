//! Windows shared memory writer â€” feeds PCM into the kernel driver ring buffer.
//!
//! Uses a file-backed mapping at C:\ProgramData\PhoneMike\ring.dat (64KB).
//! Both PC client and kernel driver map the same file â€” no named object
//! namespace or security descriptor issues.
//!
//! Layout:
//!   [0..28]  PhoneMike_SHARED_HEADER (#pragma pack 1)
//!   [28..]   ring data  (65536 - 28 = 65508 bytes)
//!
//! WriteIndex and ReadIndex are monotonically incrementing byte counters.
//! Position in ring = index % RING_CAPACITY.

#![cfg(target_os = "windows")]

use anyhow::{bail, Result};
use std::sync::atomic::{AtomicI32, Ordering};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, OPEN_ALWAYS, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_ATTRIBUTE_NORMAL,
};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_WRITE,
    PAGE_READWRITE,
};

const SHARED_MEM_SIZE: usize = 64 * 1024;
const MAGIC: u32 = 0x434D4850; // 'PHMC' LE

const HEADER_SIZE: usize = 28;
const RING_CAPACITY: usize = SHARED_MEM_SIZE - HEADER_SIZE;

// Byte offsets within the mapping
const OFF_MAGIC: usize = 0;
const OFF_SAMPLE_RATE: usize = 4;
const OFF_CHANNELS: usize = 8;
const OFF_BITS: usize = 10;
const OFF_RING_CAPACITY: usize = 12;
const OFF_WRITE_INDEX: usize = 16;
const OFF_READ_INDEX: usize = 20;
const OFF_RUNNING: usize = 24;
const OFF_RING_DATA: usize = HEADER_SIZE;

/// File path for shared memory (wide string, null-terminated)
/// C:\ProgramData\PhoneMike\ring.dat
fn shared_file_path() -> Vec<u16> {
    "C:\\ProgramData\\PhoneMike\\ring.dat\0"
        .encode_utf16()
        .collect()
}

pub struct SharedMemWriter {
    file_handle: HANDLE,
    mapping_handle: HANDLE,
    view: *mut u8,
}

unsafe impl Send for SharedMemWriter {}

impl SharedMemWriter {
    pub fn new(sample_rate: u32, channels: u16, bits_per_sample: u16) -> Result<Self> {
        // Ensure directory exists
        let dir = std::path::Path::new("C:\\ProgramData\\PhoneMike");
        if !dir.exists() {
            std::fs::create_dir_all(dir)?;
        }

        let path = shared_file_path();

        // Open or create the file
        let file_handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                0x80000000 | 0x40000000, // GENERIC_READ | GENERIC_WRITE
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                0,
            )
        };
        if file_handle == INVALID_HANDLE_VALUE {
            bail!("CreateFileW failed (err={})",
                unsafe { windows_sys::Win32::Foundation::GetLastError() });
        }

        // Create file mapping
        let mapping_handle = unsafe {
            CreateFileMappingW(
                file_handle,
                std::ptr::null(),
                PAGE_READWRITE,
                0,
                SHARED_MEM_SIZE as u32,
                std::ptr::null(), // anonymous â€” no name needed
            )
        };
        if mapping_handle == 0 {
            let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            unsafe { CloseHandle(file_handle) };
            bail!("CreateFileMappingW failed (err={})", err);
        }

        let view = unsafe { MapViewOfFile(mapping_handle, FILE_MAP_WRITE, 0, 0, SHARED_MEM_SIZE) };
        if view.Value.is_null() {
            unsafe {
                CloseHandle(mapping_handle);
                CloseHandle(file_handle);
            }
            bail!("MapViewOfFile failed");
        }

        let ptr = view.Value as *mut u8;

        // Initialize header
        unsafe {
            write_u32(ptr, OFF_MAGIC, MAGIC);
            write_u32(ptr, OFF_SAMPLE_RATE, sample_rate);
            write_u16(ptr, OFF_CHANNELS, channels);
            write_u16(ptr, OFF_BITS, bits_per_sample);
            write_u32(ptr, OFF_RING_CAPACITY, RING_CAPACITY as u32);
            write_i32_atomic(ptr, OFF_WRITE_INDEX, 0);
            write_i32_atomic(ptr, OFF_READ_INDEX, 0);
            write_i32_atomic(ptr, OFF_RUNNING, 1);
        }

        eprintln!(
            "[PhoneMike] Shared mem file opened: {} bytes ring ({} Hz, {} ch, {} bit)",
            RING_CAPACITY, sample_rate, channels, bits_per_sample
        );

        Ok(Self { file_handle, mapping_handle, view: ptr })
    }

    pub fn write(&self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let ring_base = unsafe { self.view.add(OFF_RING_DATA) };
        let write_atomic = unsafe {
            &*(self.view.add(OFF_WRITE_INDEX) as *const AtomicI32)
        };

        let wi = write_atomic.load(Ordering::Relaxed) as u32 as usize;

        let mut remaining = data;
        let mut offset = 0usize;
        let total = data.len();

        while offset < total {
            let pos = (wi + offset) % RING_CAPACITY;
            let contiguous = RING_CAPACITY - pos;
            let to_copy = (total - offset).min(contiguous);
            unsafe {
                std::ptr::copy_nonoverlapping(
                    remaining.as_ptr(),
                    ring_base.add(pos),
                    to_copy,
                );
            }
            offset += to_copy;
            remaining = &remaining[to_copy..];
        }

        write_atomic.fetch_add(total as i32, Ordering::Release);
    }

    pub fn indices(&self) -> (i32, i32) {
        let wi = unsafe { &*(self.view.add(OFF_WRITE_INDEX) as *const AtomicI32) };
        let ri = unsafe { &*(self.view.add(OFF_READ_INDEX) as *const AtomicI32) };
        (wi.load(Ordering::Relaxed), ri.load(Ordering::Relaxed))
    }

    pub fn stop(&self) {
        unsafe { write_i32_atomic(self.view, OFF_RUNNING, 0) };
    }
}

impl Drop for SharedMemWriter {
    fn drop(&mut self) {
        self.stop();
        unsafe {
            UnmapViewOfFile(windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.view as *mut _,
            });
            CloseHandle(self.mapping_handle);
            CloseHandle(self.file_handle);
        }
    }
}

// --- helpers ---

unsafe fn write_u32(base: *mut u8, off: usize, val: u32) {
    let bytes = val.to_le_bytes();
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), base.add(off), 4);
}

unsafe fn write_u16(base: *mut u8, off: usize, val: u16) {
    let bytes = val.to_le_bytes();
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), base.add(off), 2);
}

unsafe fn write_i32_atomic(base: *mut u8, off: usize, val: i32) {
    let atomic = &*(base.add(off) as *const AtomicI32);
    atomic.store(val, Ordering::SeqCst);
}
//! Pipes for the chapter 7 API exercise.
//!
//! Keep the provided types, signatures, constructors, and endpoint helpers.
//! The seven TODOs implement the ring buffer, pipe creation, and blocking I/O.
//! Internal helpers may be added within the exercise files.
//!
//! Read and write operate on already translated user buffers. Release the
//! ring-buffer borrow before yielding, and check its state again on resumption.
//! Reads fill the request unless EOF is reached; writes complete the request.
//! Zero-length requests return immediately. Broken-pipe errors and interruption
//! of a waiting operation by signals are outside this exercise's contract.

// Allow unused items, imports, and parameters in the exercise skeleton.
#![allow(dead_code, unused_imports, unused_variables)]

use super::File;
use crate::mm::UserBuffer;
use crate::sync::UPSafeCell;
use alloc::sync::{Arc, Weak};

use crate::task::suspend_current_and_run_next;

/// IPC pipe
pub struct Pipe {
    readable: bool,
    writable: bool,
    buffer: Arc<UPSafeCell<PipeRingBuffer>>,
}

impl Pipe {
    /// create readable pipe
    pub fn read_end_with_buffer(buffer: Arc<UPSafeCell<PipeRingBuffer>>) -> Self {
        Self {
            readable: true,
            writable: false,
            buffer,
        }
    }
    /// create writable pipe
    pub fn write_end_with_buffer(buffer: Arc<UPSafeCell<PipeRingBuffer>>) -> Self {
        Self {
            readable: false,
            writable: true,
            buffer,
        }
    }
}

const RING_BUFFER_SIZE: usize = 32;

#[derive(Copy, Clone, PartialEq)]
enum RingBufferStatus {
    Full,
    Empty,
    Normal,
}

/// Shared FIFO storage. Head is the next read position, tail the next write
/// position; status distinguishes full from empty when head equals tail.
pub struct PipeRingBuffer {
    arr: [u8; RING_BUFFER_SIZE],
    head: usize,
    tail: usize,
    status: RingBufferStatus,
    write_end: Option<Weak<Pipe>>,
}

impl PipeRingBuffer {
    pub fn new() -> Self {
        Self {
            arr: [0; RING_BUFFER_SIZE],
            head: 0,
            tail: 0,
            status: RingBufferStatus::Empty,
            write_end: None,
        }
    }
    pub fn set_write_end(&mut self, write_end: &Arc<Pipe>) {
        self.write_end = Some(Arc::downgrade(write_end));
    }

    /// Append one byte at the tail, preserving FIFO order.
    ///
    /// The caller guarantees available_write() > 0 and exclusive access.
    /// Advance the tail with wraparound and update Normal/Full status without
    /// changing the head, unread data, or the registered write endpoint.
    pub fn write_byte(&mut self, byte: u8) {
        todo!("fs::PipeRingBuffer::write_byte")
    }

    /// Remove and return the oldest unread byte.
    ///
    /// The caller guarantees available_read() > 0 and exclusive access.
    /// Advance the head with wraparound and update Normal/Empty status without
    /// changing the tail or the remaining unread bytes.
    pub fn read_byte(&mut self) -> u8 {
        todo!("fs::PipeRingBuffer::read_byte")
    }

    /// Return the unread byte count in 0..=RING_BUFFER_SIZE without mutation.
    /// Empty means zero and Full means the entire capacity; Normal must account
    /// for both contiguous and wrapped contents.
    pub fn available_read(&self) -> usize {
        todo!("fs::PipeRingBuffer::available_read")
    }

    /// Return the free byte count without mutation. Together with
    /// available_read(), the result must sum to RING_BUFFER_SIZE.
    pub fn available_write(&self) -> usize {
        todo!("fs::PipeRingBuffer::available_write")
    }

    /// The write endpoint must have been registered by make_pipe(). Its Weak
    /// reference expires only after every strong reference to that endpoint is
    /// released; this does not imply that the buffer has already been drained.
    pub fn all_write_ends_closed(&self) -> bool {
        self.write_end.as_ref().unwrap().upgrade().is_none()
    }
}

/// Create an empty pipe and return (read_end, write_end), in that order.
///
/// Both endpoints share exactly one ring buffer. Use the provided constructors
/// for their read-only/write-only permissions, and register the write endpoint
/// with set_write_end() before returning. The buffer must not own a strong
/// reference to its write endpoint. This function does not allocate descriptors.
pub fn make_pipe() -> (Arc<Pipe>, Arc<Pipe>) {
    todo!("fs::make_pipe")
}

impl File for Pipe {
    fn readable(&self) -> bool {
        self.readable
    }
    fn writable(&self) -> bool {
        self.writable
    }
    /// Read FIFO bytes into the translated buffer in slice order.
    ///
    /// The caller supplies a readable endpoint and a UserBuffer produced by the
    /// existing translation helpers. A zero-length request returns 0 immediately.
    /// Otherwise keep reading until the request is full or the buffer is empty
    /// and all write-end references have been released. Only EOF permits a short
    /// read; drain residual bytes even after the write endpoint has closed.
    /// Leave the user buffer beyond the returned byte count unchanged.
    ///
    /// When empty with writers still alive, release the ring-buffer borrow,
    /// suspend_current_and_run_next(), and recheck on resumption. Do not consume
    /// more bytes than requested, translate addresses again, or allocate an fd.
    fn read(&self, buf: UserBuffer) -> usize {
        todo!("fs::Pipe::read")
    }

    /// Write all bytes from the translated buffer in slice order and return its
    /// length. A zero-length request returns 0 immediately, even when full.
    ///
    /// The caller supplies a writable endpoint and a UserBuffer produced by the
    /// existing translation helpers. Write only into available capacity; when
    /// full, release the ring-buffer borrow before suspend_current_and_run_next()
    /// and recheck on resumption. Preserve progress across yields and never
    /// overwrite unread bytes. Completion of a nonempty write depends on readers
    /// making room. Tracking closed read endpoints or raising EPIPE/SIGPIPE is
    /// not required by this interface.
    fn write(&self, buf: UserBuffer) -> usize {
        todo!("fs::Pipe::write")
    }
}

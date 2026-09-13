//! Sv39 page tables and address translation for chapter 4.
//!
//! Mappings use 4 KiB pages. An owning page table keeps its root and intermediate
//! frames in `frames`; a view from `from_token` does not own those frames.
//! Mapped data frames belong to memory areas.
//! Internal helper functions may be designed freely.

// Allow unused items, imports, and parameters in the exercise skeleton.
#![allow(dead_code, unused_imports, unused_variables)]

use super::{frame_alloc, FrameTracker, PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum};
use alloc::vec;
use alloc::vec::Vec;
use bitflags::*;

bitflags! {
    /// page table entry flags
    pub struct PTEFlags: u8 {
        /// Valid
        const V = 1 << 0;
        /// Readable
        const R = 1 << 1;
        /// Writable
        const W = 1 << 2;
        /// eXecutable
        const X = 1 << 3;
        /// User
        const U = 1 << 4;
        /// Global
        const G = 1 << 5;
        /// Accessed
        const A = 1 << 6;
        /// Dirty
        const D = 1 << 7;
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
/// page table entry structure
pub struct PageTableEntry {
    /// bits of page table entry
    pub bits: usize,
}

impl PageTableEntry {
    /// Create a new page table entry
    pub fn new(ppn: PhysPageNum, flags: PTEFlags) -> Self {
        PageTableEntry {
            bits: ppn.0 << 10 | flags.bits as usize,
        }
    }
    /// Create an empty page table entry
    pub fn empty() -> Self {
        PageTableEntry { bits: 0 }
    }
    /// Get the physical page number from the page table entry
    pub fn ppn(&self) -> PhysPageNum {
        (self.bits >> 10 & ((1usize << 44) - 1)).into()
    }
    /// Get the flags from the page table entry
    pub fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits(self.bits as u8).unwrap()
    }
    /// The page pointered by page table entry is valid?
    pub fn is_valid(&self) -> bool {
        (self.flags() & PTEFlags::V) != PTEFlags::empty()
    }
    /// The page pointered by page table entry is readable?
    pub fn readable(&self) -> bool {
        (self.flags() & PTEFlags::R) != PTEFlags::empty()
    }
    /// The page pointered by page table entry is writable?
    pub fn writable(&self) -> bool {
        (self.flags() & PTEFlags::W) != PTEFlags::empty()
    }
    /// The page pointered by page table entry is executable?
    pub fn executable(&self) -> bool {
        (self.flags() & PTEFlags::X) != PTEFlags::empty()
    }
}

/// page table structure
pub struct PageTable {
    root_ppn: PhysPageNum,
    frames: Vec<FrameTracker>,
}

impl PageTable {
    /// Create a new page table
    pub fn new() -> Self {
        let frame = frame_alloc().unwrap();
        PageTable {
            root_ppn: frame.ppn,
            frames: vec![frame],
        }
    }
    /// Temporarily used to get arguments from user space.
    pub fn from_token(satp: usize) -> Self {
        Self {
            root_ppn: PhysPageNum::from(satp & ((1usize << 44) - 1)),
            frames: Vec::new(),
        }
    }
    // Implementation hints:
    //
    // fn find_pte_create(&mut self, vpn: VirtPageNum) -> Option<&mut PageTableEntry>;
    // fn find_pte(&self, vpn: VirtPageNum) -> Option<&mut PageTableEntry>;

    /// Todo: Map a virtual page to the specified physical page.
    ///
    /// Inputs: `vpn` is the virtual page, `ppn` is its physical backing page,
    /// and `flags` specifies the page table entry's access permissions.
    /// Output: `Some(())` when the mapping is established, or `None` if the
    /// virtual page is already mapped or a required page table frame cannot
    /// be allocated.
    /// Constraints: Use a 4 KiB Sv39 mapping with `V` and the requested flags.
    /// This page table owns its intermediate page table frames through `frames`;
    /// ownership of the mapped data frame remains with the caller.
    pub fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) -> Option<()> {
        todo!("mm::PageTable::map")
    }

    /// Todo: Remove the mapping for a virtual page.
    ///
    /// Inputs: `vpn` identifies a currently mapped virtual page.
    /// Output: `()`; the specified virtual page no longer has a valid mapping.
    /// Constraints: Other mappings remain valid. Data-frame allocation and
    /// reclamation belong to the caller.
    pub fn unmap(&mut self, vpn: VirtPageNum) {
        todo!("mm::PageTable::unmap")
    }

    /// Todo: Look up the final-level page table entry for a virtual page.
    ///
    /// Inputs: `vpn` is a virtual page number in this page table.
    /// Output: A copy of the final-level entry, or `None` if no such entry
    /// is reachable through the page table.
    /// Constraints: Use the Sv39 layout with 4 KiB pages. Preserve the stored
    /// entry's flags; returning an entry does not imply that its valid bit is set
    /// or that it permits user access.
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        todo!("mm::PageTable::translate")
    }

    /// Todo: Translate a user address with the requested access permissions.
    ///
    /// Inputs: `addr` is a virtual byte address; `permission` contains the
    /// required access flags, such as `R` or `W`.
    /// Output: `Some(physical_address)` for a permitted access, or `None`
    /// when the address or mapping does not permit that access.
    /// Constraints: The address must be a canonical Sv39 address. The mapping
    /// must have `V`, `U`, and every requested permission bit set.
    /// The physical address must retain the original page offset.
    pub fn translate_user(&self, addr: usize, permission: PTEFlags) -> Option<PhysAddr> {
        todo!("mm::PageTable::translate_user")
    }

    /// get the token from the page table
    pub fn token(&self) -> usize {
        8usize << 60 | self.root_ppn.0
    }
}

/// Todo: Expose a user virtual buffer as slices of its physical backing memory.
///
/// Inputs: `token` identifies the user page table, `ptr` is the starting user
/// virtual address, and `len` is the byte length. The caller supplies a mapped
/// range whose address calculation does not overflow.
/// Output: Mutable byte slices in virtual address order, covering exactly
/// `[ptr, ptr + len)` without copying its contents.
/// Constraints: Support page offsets and physically noncontiguous pages.
/// Every slice must stay within its backing page, and the backing frames
/// must remain valid while the returned slices are used.
pub fn translated_byte_buffer(token: usize, ptr: *const u8, len: usize) -> Vec<&'static mut [u8]> {
    todo!("mm::translated_byte_buffer")
}

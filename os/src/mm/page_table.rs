//! Sv39 page tables and address translation for chapter 4.
//!
//! Mappings use 4 KiB pages. An owning page table keeps its root and intermediate
//! frames in `frames`; a view from `from_token` does not own those frames.
//! Mapped data frames belong to memory areas.
//! Internal helper functions may be designed freely.

// Allow unused items, imports, and parameters in the exercise skeleton.
#![allow(dead_code, unused_imports, unused_variables)]

use super::{frame_alloc, FrameTracker, PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
use crate::config::PAGE_SIZE;
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
    fn find_pte_create(&mut self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let mut ppn = self.root_ppn;
        for (level, index) in vpn.indexes().into_iter().enumerate() {
            let pte = &mut ppn.get_pte_array()[index];
            if level == 2 {
                return Some(pte);
            }
            if !pte.is_valid() {
                let frame = frame_alloc()?;
                *pte = PageTableEntry::new(frame.ppn, PTEFlags::V);
                self.frames.push(frame);
            } else if pte
                .flags()
                .intersects(PTEFlags::R | PTEFlags::W | PTEFlags::X)
            {
                // This interface only manages 4 KiB pages, not upper-level leaves.
                return None;
            }
            ppn = pte.ppn();
        }
        None
    }

    // Return the final table and index without allocating or borrowing its entry.
    fn find_pte(&self, vpn: VirtPageNum) -> Option<(PhysPageNum, usize)> {
        let mut ppn = self.root_ppn;
        let indexes = vpn.indexes();
        for &index in &indexes[..2] {
            let pte = ppn.get_pte_array()[index];
            if !pte.is_valid()
                || pte
                    .flags()
                    .intersects(PTEFlags::R | PTEFlags::W | PTEFlags::X)
            {
                return None;
            }
            ppn = pte.ppn();
        }
        Some((ppn, indexes[2]))
    }

    /// Map a virtual page to the specified physical page.
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
        let pte = self.find_pte_create(vpn)?;
        if pte.is_valid() {
            return None;
        }
        *pte = PageTableEntry::new(ppn, flags | PTEFlags::V);
        Some(())
    }

    /// Remove the mapping for a virtual page.
    ///
    /// Inputs: `vpn` identifies a currently mapped virtual page.
    /// Output: `()`; the specified virtual page no longer has a valid mapping.
    /// Constraints: Other mappings remain valid. Data-frame allocation and
    /// reclamation belong to the caller.
    pub fn unmap(&mut self, vpn: VirtPageNum) {
        let (ppn, index) = self.find_pte(vpn).expect("Missing page table path");
        let pte = &mut ppn.get_pte_array()[index];
        assert!(pte.is_valid(), "Cannot unmap an invalid page");
        *pte = PageTableEntry::empty();
    }

    /// Look up the final-level page table entry for a virtual page.
    ///
    /// Inputs: `vpn` is a virtual page number in this page table.
    /// Output: A copy of the final-level entry, or `None` if no such entry
    /// is reachable through the page table.
    /// Constraints: Use the Sv39 layout with 4 KiB pages. Preserve the stored
    /// entry's flags; returning an entry does not imply that its valid bit is set
    /// or that it permits user access.
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        let (ppn, index) = self.find_pte(vpn)?;
        Some(ppn.get_pte_array()[index])
    }

    /// Translate a user address with the requested access permissions.
    ///
    /// Inputs: `addr` is a virtual byte address; `permission` contains the
    /// required access flags, such as `R` or `W`.
    /// Output: `Some(physical_address)` for a permitted access, or `None`
    /// when the address or mapping does not permit that access.
    /// Constraints: The address must be a canonical Sv39 address. The mapping
    /// must have `V`, `U`, and every requested permission bit set.
    /// The physical address must retain the original page offset.
    pub fn translate_user(&self, addr: usize, permission: PTEFlags) -> Option<PhysAddr> {
        let va = VirtAddr::from(addr);
        // Conversion back to usize sign-extends Sv39 bit 38. Reject addresses
        // whose high bits would otherwise be silently truncated by VirtAddr.
        if usize::from(va) != addr {
            return None;
        }
        let pte = self.translate(va.floor())?;
        if !pte.flags().contains(PTEFlags::V | PTEFlags::U | permission) {
            return None;
        }
        let page_start = PhysAddr::from(pte.ppn());
        Some(PhysAddr::from(page_start.0 + va.page_offset()))
    }

    /// get the token from the page table
    pub fn token(&self) -> usize {
        8usize << 60 | self.root_ppn.0
    }
}

/// Expose a user virtual buffer as slices of its physical backing memory.
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
    let page_table = PageTable::from_token(token);
    let mut buffers = Vec::new();
    let mut start = ptr as usize;
    let end = start
        .checked_add(len)
        .expect("User buffer address overflow");
    while start < end {
        let va = VirtAddr::from(start);
        let pte = page_table
            .translate(va.floor())
            .expect("Unmapped user buffer");
        assert!(pte.is_valid(), "Invalid user buffer page");
        let offset = va.page_offset();
        let count = (PAGE_SIZE - offset).min(end - start);
        buffers.push(&mut pte.ppn().get_bytes_array()[offset..offset + count]);
        start += count;
    }
    buffers
}

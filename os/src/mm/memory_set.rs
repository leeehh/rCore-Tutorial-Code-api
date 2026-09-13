//! Logical memory areas and address spaces for chapter 4.
//!
//! A memory area describes a half-open virtual page range and its permissions.
//! Framed areas own their data frames; identical mappings do not own the
//! physical memory they map. Area metadata and page table mappings must agree.
//! Internal helper functions may be designed freely.

// Allow unused items, imports, and parameters in the exercise skeleton.
#![allow(dead_code, unused_imports, unused_variables)]

use super::{frame_alloc, FrameTracker};
use super::{PTEFlags, PageTable, PageTableEntry};
use super::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
use super::{StepByOne, VPNRange};
use crate::config::{
    KERNEL_STACK_SIZE, MEMORY_END, PAGE_SIZE, TRAMPOLINE, TRAP_CONTEXT_BASE, USER_STACK_SIZE,
};
use crate::sync::UPSafeCell;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::asm;
use lazy_static::*;
use riscv::register::satp;

extern "C" {
    fn stext();
    fn etext();
    fn srodata();
    fn erodata();
    fn sdata();
    fn edata();
    fn sbss_with_stack();
    fn ebss();
    fn ekernel();
    fn strampoline();
}

#[derive(Copy, Clone, PartialEq, Debug)]
/// map type for memory set: identical or framed
pub enum MapType {
    Identical,
    Framed,
}

bitflags! {
    /// map permission corresponding to that in pte: `R W X U`
    pub struct MapPermission: u8 {
        ///Readable
        const R = 1 << 1;
        ///Writable
        const W = 1 << 2;
        ///Excutable
        const X = 1 << 3;
        ///Accessible in U mode
        const U = 1 << 4;
    }
}

/// map area structure, controls a contiguous piece of virtual memory
pub struct MapArea {
    vpn_range: VPNRange,
    data_frames: BTreeMap<VirtPageNum, FrameTracker>,
    map_type: MapType,
    map_perm: MapPermission,
}

impl MapArea {
    pub fn new(
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_type: MapType,
        map_perm: MapPermission,
    ) -> Self {
        let start_vpn: VirtPageNum = start_va.floor();
        let end_vpn: VirtPageNum = end_va.ceil();
        Self {
            vpn_range: VPNRange::new(start_vpn, end_vpn),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
        }
    }
    /// Map one page with the area's mapping type and permissions.
    pub fn map_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) -> Option<()> {
        let ppn: PhysPageNum;
        match self.map_type {
            MapType::Identical => {
                ppn = PhysPageNum(vpn.0);
            }
            MapType::Framed => {
                let frame = frame_alloc()?;
                ppn = frame.ppn;
                self.data_frames.insert(vpn, frame);
            }
        }
        let pte_flags = PTEFlags::from_bits(self.map_perm.bits).unwrap();
        page_table.map(vpn, ppn, pte_flags)
    }
    #[allow(unused)]
    pub fn unmap_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        if self.map_type == MapType::Framed {
            self.data_frames.remove(&vpn);
        }
        page_table.unmap(vpn);
    }
    /// Map all pages in this area.
    pub fn map(&mut self, page_table: &mut PageTable) -> Option<()> {
        for vpn in self.vpn_range {
            self.map_one(page_table, vpn)?;
        }
        Some(())
    }
    #[allow(unused)]
    pub fn unmap(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.unmap_one(page_table, vpn);
        }
    }
    #[allow(unused)]
    pub fn shrink_to(&mut self, page_table: &mut PageTable, new_end: VirtPageNum) {
        for vpn in VPNRange::new(new_end, self.vpn_range.get_end()) {
            self.unmap_one(page_table, vpn)
        }
        self.vpn_range = VPNRange::new(self.vpn_range.get_start(), new_end);
    }
    #[allow(unused)]
    /// Extend this area to the given end page.
    pub fn append_to(&mut self, page_table: &mut PageTable, new_end: VirtPageNum) -> Option<()> {
        for vpn in VPNRange::new(self.vpn_range.get_end(), new_end) {
            self.map_one(page_table, vpn)?;
        }
        self.vpn_range = VPNRange::new(self.vpn_range.get_start(), new_end);
        Some(())
    }
    /// data: start-aligned but maybe with shorter length
    /// assume that all frames were cleared before
    pub fn copy_data(&mut self, page_table: &mut PageTable, data: &[u8]) {
        assert_eq!(self.map_type, MapType::Framed);
        let mut start: usize = 0;
        let mut current_vpn = self.vpn_range.get_start();
        let len = data.len();
        loop {
            let src = &data[start..len.min(start + PAGE_SIZE)];
            let dst = &mut page_table
                .translate(current_vpn)
                .unwrap()
                .ppn()
                .get_bytes_array()[..src.len()];
            dst.copy_from_slice(src);
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            current_vpn.step();
        }
    }
}

/// address space
pub struct MemorySet {
    page_table: PageTable,
    areas: Vec<MapArea>,
}

lazy_static! {
    /// Todo: Initialize the shared kernel address space.
    ///
    /// Inputs: The linker symbols above and the memory layout constants.
    /// Output: An `Arc<UPSafeCell<MemorySet>>` containing the kernel's Sv39
    /// address space, ready for the supplied `activate()` method.
    /// Constraints: Identity-map text as RX, rodata as R, and data, bss, and
    /// physical memory from `ekernel` to `MEMORY_END` as RW, all without U.
    /// Map `TRAMPOLINE` to the physical page at `strampoline` with RX and no U.
    /// Keep the trampoline outside `areas`. Task-specific kernel stacks are
    /// added by the task module.
    pub static ref KERNEL_SPACE: Arc<UPSafeCell<MemorySet>> = {
        todo!("mm::KERNEL_SPACE")
    };
}

impl MemorySet {
    /// Create a new empty `MemorySet`.
    pub fn new_bare() -> Self {
        Self {
            page_table: PageTable::new(),
            areas: Vec::new(),
        }
    }
    /// Get the page table token
    pub fn token(&self) -> usize {
        self.page_table.token()
    }

    // Implementation hints:
    //
    // fn push(&mut self, mut map_area: MapArea, data: Option<&[u8]>) -> Option<()>;
    // fn map_trampoline(&mut self);
    // pub fn new_kernel() -> Self;

    /// Todo: Add an area backed by newly allocated data frames.
    ///
    /// Inputs: `[start_va, end_va)` is a virtual address range with no existing
    /// mappings in its covered pages; `permission` specifies the area permissions.
    /// Output: `Some(())` when the area is mapped and recorded, or `None` if
    /// the required frames cannot be allocated.
    /// Constraints: Cover `[start_va.floor(), end_va.ceil())` with 4 KiB pages.
    /// New data pages contain zeros and remain owned by this framed area.
    /// The area's range and permissions must match its page table entries.
    pub fn insert_framed_area(
        &mut self,
        start_va: VirtAddr,
        end_va: VirtAddr,
        permission: MapPermission,
    ) -> Option<()> {
        todo!("mm::MemorySet::insert_framed_area")
    }

    /// Todo: Build a user address space for an ELF application.
    ///
    /// Inputs: `elf_data` contains a valid application ELF image from the loader.
    /// Output: `(memory_set, user_stack_top, entry_point)` for the application.
    /// Constraints: Loadable segments contain their file data followed by zeros,
    /// with U and the R/W/X permissions specified by the ELF image.
    /// The RWU user stack has `USER_STACK_SIZE` bytes and follows the highest
    /// segment page with one unmapped guard page in between. An initially empty
    /// RWU heap area starts at the stack top for the supplied `sbrk` path.
    /// Map `[TRAP_CONTEXT_BASE, TRAMPOLINE)` as framed RW memory without U.
    /// Map `TRAMPOLINE` to `strampoline` with RX and no U, outside `areas`.
    /// Each application owns its data frames independently.
    pub fn from_elf(elf_data: &[u8]) -> (Self, usize, usize) {
        todo!("mm::MemorySet::from_elf")
    }

    /// Change page table by writing satp CSR Register.
    pub fn activate(&self) {
        let satp = self.page_table.token();
        unsafe {
            satp::write(satp);
            asm!("sfence.vma");
        }
    }
    /// Translate a virtual page number to a page table entry
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.page_table.translate(vpn)
    }

    /// Check whether any page in `[start, end)` has a valid mapping.
    pub fn has_mapped_pages(&self, start: VirtPageNum, end: VirtPageNum) -> bool {
        VPNRange::new(start, end).into_iter().any(|vpn| {
            self.page_table
                .translate(vpn)
                .map_or(false, |pte| pte.is_valid())
        })
    }

    /// Check whether any page in `[start, end)` lacks a valid mapping.
    pub fn has_unmapped_pages(&self, start: VirtPageNum, end: VirtPageNum) -> bool {
        VPNRange::new(start, end).into_iter().any(|vpn| {
            self.page_table
                .translate(vpn)
                .map_or(true, |pte| !pte.is_valid())
        })
    }

    /// Todo: Remove a complete framed area.
    ///
    /// Inputs: `start` and `end` are the virtual page bounds of the area.
    /// Output: `Some(())` when the matching area is removed, or `None` when
    /// no area has exactly those bounds.
    /// Constraints: The whole area is the unit of removal. Its mappings and
    /// metadata are removed, and its owned data frames are released.
    /// Other areas retain their mappings and contents.
    pub fn remove_framed_area(&mut self, start: VirtPageNum, end: VirtPageNum) -> Option<()> {
        todo!("mm::MemorySet::remove_framed_area")
    }

    /// Todo: Shrink an existing area to a new end address.
    ///
    /// Inputs: `start` identifies the area's first page; `new_end` is the
    /// requested end address within the area's current address range.
    /// Output: `true` when the area is found and resized, or `false` if absent.
    /// Constraints: The resulting page range ends at `new_end.ceil()`.
    /// Retained pages preserve their data and permissions. Truncated pages
    /// are unmapped and their owned data frames are released.
    #[allow(unused)]
    pub fn shrink_to(&mut self, start: VirtAddr, new_end: VirtAddr) -> bool {
        todo!("mm::MemorySet::shrink_to")
    }

    /// Todo: Extend an existing area to a new end address.
    ///
    /// Inputs: `start` identifies the area's first page; `new_end` is at or
    /// beyond its current end, with no conflicting mappings in the added range.
    /// Output: `true` when the area is extended, or `false` if the area is
    /// absent or the required frames cannot be allocated.
    /// Constraints: The resulting page range ends at `new_end.ceil()`.
    /// Existing data and permissions are preserved; new framed pages contain
    /// zeros and have the same permissions as the area.
    #[allow(unused)]
    pub fn append_to(&mut self, start: VirtAddr, new_end: VirtAddr) -> bool {
        todo!("mm::MemorySet::append_to")
    }
}

/// Return (bottom, top) of a kernel stack in kernel space.
pub fn kernel_stack_position(app_id: usize) -> (usize, usize) {
    let top = TRAMPOLINE - app_id * (KERNEL_STACK_SIZE + PAGE_SIZE);
    let bottom = top - KERNEL_STACK_SIZE;
    (bottom, top)
}

/// remap test in kernel space
#[allow(unused)]
pub fn remap_test() {
    let mut kernel_space = KERNEL_SPACE.exclusive_access();
    let mid_text: VirtAddr = ((stext as usize + etext as usize) / 2).into();
    let mid_rodata: VirtAddr = ((srodata as usize + erodata as usize) / 2).into();
    let mid_data: VirtAddr = ((sdata as usize + edata as usize) / 2).into();
    assert!(!kernel_space
        .page_table
        .translate(mid_text.floor())
        .unwrap()
        .writable(),);
    assert!(!kernel_space
        .page_table
        .translate(mid_rodata.floor())
        .unwrap()
        .writable(),);
    assert!(!kernel_space
        .page_table
        .translate(mid_data.floor())
        .unwrap()
        .executable(),);
    println!("remap_test passed!");
}

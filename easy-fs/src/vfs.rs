//! File, directory, and hard-link management for the chapter 6 API exercise.
//!
//! The supplied disk layout, bitmaps, caches, and private helpers are available
//! to all nine fixed interfaces. Directory entries with inode ID 0 are empty;
//! the root inode itself has ID 0. Names fit in 27 bytes, and this exercise uses
//! single-component names rather than path traversal. File writes fit the disk
//! layout and have sufficient free blocks; no allocation rollback is required.
//!
//! Serialize operations with the filesystem lock. Do not reacquire that lock
//! through another public operation while holding it. Directory and target
//! inodes can share a cache block: release one cache guard before acquiring
//! another guard for the same block. Release cache guards before syncing caches.
//!
//! Internal helpers may be designed freely within the supplied interfaces.

// Allow unused items, imports, and parameters in the exercise skeleton.
#![allow(dead_code, unused_imports, unused_variables)]

use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};
/// Virtual filesystem layer over easy-fs
pub struct Inode {
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }
    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over a disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Todo: Query the identity and current metadata of this inode.
    ///
    /// Inputs: `self` identifies a live inode in this filesystem.
    /// Output: `(inode_id, nlink, is_directory)` from its current disk metadata.
    /// Constraints: The inode ID agrees with its disk position. Read the stored
    /// hard-link count, not an Arc reference count, without changing the inode.
    pub fn stat(&self) -> (u32, u32, bool) {
        todo!("easy_fs::Inode::stat")
    }
    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        self.find_dirent(name, disk_inode)
            .map(|(_, inode_id)| inode_id)
    }
    /// Find the byte offset and inode ID of a live directory entry.
    fn find_dirent(&self, name: &str, disk_inode: &DiskInode) -> Option<(usize, u32)> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            // Inode zero belongs to the root; empty entries use it as a marker.
            if dirent.inode_id() != 0 && dirent.name() == name {
                return Some((DIRENT_SZ * i, dirent.inode_id()));
            }
        }
        None
    }
    /// Todo: Find a named entry in this directory.
    ///
    /// Inputs: `self` is a directory; `name` is a single-component file name.
    /// Output: An `Arc<Inode>` for the entry, or `None` if it does not exist.
    /// Constraints: Skip empty entries. The result refers to the existing disk
    /// inode on the same filesystem and device; no inode or file data is copied.
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        todo!("easy_fs::Inode::find")
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }
    /// Insert a name for an allocated inode, reusing an unlinked entry if possible.
    fn append_dirent(
        &self,
        name: &str,
        inode_id: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        let mut offset = disk_inode.size as usize;
        let mut entry = DirEntry::empty();
        for pos in (0..offset).step_by(DIRENT_SZ) {
            assert_eq!(
                disk_inode.read_at(pos, entry.as_bytes_mut(), &self.block_device),
                DIRENT_SZ,
            );
            if entry.inode_id() == 0 {
                offset = pos;
                break;
            }
        }
        if offset == disk_inode.size as usize {
            self.increase_size((offset + DIRENT_SZ) as u32, disk_inode, fs);
        }
        let dirent = DirEntry::new(name, inode_id);
        assert_eq!(
            disk_inode.write_at(offset, dirent.as_bytes(), &self.block_device),
            DIRENT_SZ,
        );
    }
    /// Todo: Create an empty regular file in this directory.
    ///
    /// Inputs: `self` is a directory and `name` is a valid name of at most 27
    /// bytes. Inode and data-block allocation have sufficient free space.
    /// Output: The newly created inode, or `None` when the name already exists.
    /// Constraints: A new inode has size 0 and nlink 1. Register exactly one
    /// directory entry, reusing an empty slot when available, and synchronize
    /// changes. A duplicate name leaves the existing file and directory intact.
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        todo!("easy_fs::Inode::create")
    }
    /// Todo: Add another name for an existing regular file in this directory.
    ///
    /// Inputs: `old_name` names the source and `new_name` is at most 27 bytes.
    /// Output: `Some(())` on success; `None` for a non-directory receiver,
    /// missing or non-file source, existing destination, or link-count overflow.
    /// An empty new name, NUL, '/', '.', or '..' is also rejected.
    /// Constraints: Both names refer to the same inode ID and file data. Add one
    /// directory entry and increment stored nlink once, synchronizing changes.
    /// Rejected requests change neither directory entries nor the link count.
    pub fn link(&self, old_name: &str, new_name: &str) -> Option<()> {
        todo!("easy_fs::Inode::link")
    }
    /// Todo: Remove a regular file's name from this directory.
    ///
    /// Inputs: `name` identifies the directory entry to remove.
    /// Output: `Some(())` on success; `None` for a non-directory receiver,
    /// missing or non-file target, or an invalid zero link count.
    /// Constraints: Empty the entry without shrinking the directory; decrement
    /// nlink once. Remaining links keep the data intact. On the last link,
    /// reclaim data and indirect blocks and free the inode bitmap entry.
    /// Synchronize changes; failures leave the filesystem unchanged. Final
    /// unlink reclaims immediately; callers do not reuse old handles afterward.
    pub fn unlink(&self, name: &str) -> Option<()> {
        todo!("easy_fs::Inode::unlink")
    }
    /// Todo: List the live names in this directory.
    ///
    /// Inputs: `self` is a directory.
    /// Output: Names in directory-entry order, one per live entry.
    /// Constraints: Skip empty slots. Different hard-link names remain separate
    /// entries even when they share an inode ID. Do not change the directory.
    pub fn ls(&self) -> Vec<String> {
        todo!("easy_fs::Inode::ls")
    }
    /// Todo: Read inode data at an explicit byte offset.
    ///
    /// Inputs: `offset` is the starting position; `buf` is the destination.
    /// The requested range calculation does not overflow.
    /// Output: The number of bytes read, limited by the buffer and file end;
    /// return 0 for an empty buffer or an offset at or beyond EOF.
    /// Constraints: Only the returned prefix of `buf` is overwritten. Preserve
    /// file data and metadata; this layer has no per-open read/write offset.
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        todo!("easy_fs::Inode::read_at")
    }
    /// Todo: Write inode data at an explicit byte offset.
    ///
    /// Inputs: `buf` is the source; `offset + buf.len()` fits the supported
    /// file capacity and does not overflow. Free blocks are sufficient.
    /// Output: The number of bytes written, equal to `buf.len()`.
    /// Constraints: Extend the file when needed, allocating data and index
    /// blocks through the supplied helpers. Do not shrink it or change its
    /// inode ID or nlink. Preserve existing data outside the written range
    /// and synchronize the changes before returning.
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        todo!("easy_fs::Inode::write_at")
    }
    /// Todo: Truncate a regular file to zero bytes.
    ///
    /// Inputs: `self` identifies a live regular file, possibly already empty.
    /// Output: `()`; file size becomes 0 and its data and index blocks are freed.
    /// Constraints: Keep the inode allocated, preserving its ID, type, nlink,
    /// and directory entries. All hard-link names observe the empty file.
    /// Synchronize the changes; truncation does not reset OS-level offsets.
    pub fn clear(&self) {
        todo!("easy_fs::Inode::clear")
    }
    /// Shared data-block reclamation for truncation and final unlink.
    fn clear_inode_data(&self, disk_inode: &mut DiskInode, fs: &mut MutexGuard<EasyFileSystem>) {
        let size = disk_inode.size;
        let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
        assert_eq!(
            data_blocks_dealloc.len(),
            DiskInode::total_blocks(size) as usize
        );
        for data_block in data_blocks_dealloc {
            fs.dealloc_data(data_block);
        }
    }
}

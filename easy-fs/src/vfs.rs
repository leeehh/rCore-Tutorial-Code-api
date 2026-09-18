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
    /// Return the inode ID, hard-link count, and whether this is a directory.
    pub fn stat(&self) -> (u32, u32, bool) {
        let fs = self.fs.lock();
        let inode_id = fs.get_disk_inode_id(self.block_id as u32, self.block_offset);
        self.read_disk_inode(|disk_inode| (inode_id, disk_inode.nlink, disk_inode.is_dir()))
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
    /// Find inode under current inode by name
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
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
    /// Create inode under current inode by name
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        if self.read_disk_inode(op).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = fs.alloc_inode();
        // initialize inode
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });
        self.modify_disk_inode(|root_inode| {
            self.append_dirent(name, new_inode_id, root_inode, &mut fs);
        });

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock automatically by compiler
    }
    /// Add another name for an existing regular file in this directory.
    /// No inode or file data is copied; directory entries share the inode ID.
    pub fn link(&self, old_name: &str, new_name: &str) -> Option<()> {
        if new_name.is_empty()
            || new_name.as_bytes().contains(&0)
            || new_name.contains('/')
            || new_name == "."
            || new_name == ".."
        {
            return None;
        }
        // Keep lookup and insertion under one filesystem lock. Calling find()
        // here would try to acquire the same non-reentrant lock a second time.
        let mut fs = self.fs.lock();
        let inode_id = self.read_disk_inode(|disk_inode| {
            if !disk_inode.is_dir() || self.find_inode_id(new_name, disk_inode).is_some() {
                return None;
            }
            self.find_inode_id(old_name, disk_inode)
        })?;
        let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
        let inode_cache = get_block_cache(block_id as usize, Arc::clone(&self.block_device));
        let nlink = inode_cache
            .lock()
            .read(block_offset, |disk_inode: &DiskInode| {
                if disk_inode.is_file() {
                    disk_inode.nlink.checked_add(1)
                } else {
                    None
                }
            })?;
        self.modify_disk_inode(|disk_inode| {
            self.append_dirent(new_name, inode_id, disk_inode, &mut fs);
        });
        inode_cache
            .lock()
            .modify(block_offset, |disk_inode: &mut DiskInode| {
                disk_inode.nlink = nlink;
            });
        block_cache_sync_all();
        Some(())
    }
    /// Remove a name and reclaim the inode and data after the final hard link.
    pub fn unlink(&self, name: &str) -> Option<()> {
        let mut fs = self.fs.lock();
        let (offset, inode_id) = self.read_disk_inode(|disk_inode| {
            if disk_inode.is_dir() {
                self.find_dirent(name, disk_inode)
            } else {
                None
            }
        })?;
        let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
        let inode_cache = get_block_cache(block_id as usize, Arc::clone(&self.block_device));
        let nlink = inode_cache
            .lock()
            .read(block_offset, |disk_inode: &DiskInode| {
                if disk_inode.is_file() {
                    disk_inode.nlink.checked_sub(1)
                } else {
                    None
                }
            })?;
        self.modify_disk_inode(|disk_inode| {
            assert_eq!(
                disk_inode.write_at(offset, DirEntry::empty().as_bytes(), &self.block_device),
                DIRENT_SZ,
            );
        });
        // Release the directory's cache lock before locking the target inode:
        // both inodes may occupy the same disk block.
        inode_cache
            .lock()
            .modify(block_offset, |disk_inode: &mut DiskInode| {
                disk_inode.nlink = nlink;
                if nlink == 0 {
                    self.clear_inode_data(disk_inode, &mut fs);
                }
            });
        if nlink == 0 {
            fs.dealloc_inode(inode_id);
        }
        block_cache_sync_all();
        Some(())
    }
    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                if dirent.inode_id() != 0 {
                    v.push(String::from(dirent.name()));
                }
            }
            v
        })
    }
    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            self.clear_inode_data(disk_inode, &mut fs);
        });
        block_cache_sync_all();
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

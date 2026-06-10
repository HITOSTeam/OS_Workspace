//! Virtual filesystem layer for ext4

use super::error::{Ext4Error, Result};
use super::ext4::Ext4FileSystem;
use super::layout::*;
use super::{BLOCK_SZ, BlockDevice, get_block_cache};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cmp::max;
use lazy_static::lazy_static;
use spin::Mutex;

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

type DirIndex = BTreeMap<String, (u32, u8)>;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct InodeCacheKey {
    inode_num: u32,
    device_id: usize,
}

fn device_id(block_device: &Arc<dyn BlockDevice>) -> usize {
    Arc::as_ptr(block_device) as *const () as usize
}

fn inode_cache_key(inode_num: u32, block_device: &Arc<dyn BlockDevice>) -> InodeCacheKey {
    InodeCacheKey {
        inode_num,
        device_id: device_id(block_device),
    }
}

const DIR_INDEX_CACHE_MAX: usize = 64;
const EXTENTS_CACHE_MAX: usize = 256;
const EXTENTS_IN_INODE: usize = (15 * 4 - 12) / 12;
const EXTENTS_PER_NODE: usize = (BLOCK_SZ - 12) / 12;

lazy_static! {
    static ref DIR_INDEX_CACHE: Mutex<BTreeMap<InodeCacheKey, Arc<Mutex<DirIndex>>>> =
        Mutex::new(BTreeMap::new());
    static ref EXTENTS_CACHE: Mutex<BTreeMap<InodeCacheKey, Arc<Vec<Ext4Extent>>>> =
        Mutex::new(BTreeMap::new());
}

fn dir_index_cached(
    inode_num: u32,
    block_device: &Arc<dyn BlockDevice>,
) -> Option<Arc<Mutex<DirIndex>>> {
    let key = inode_cache_key(inode_num, block_device);
    DIR_INDEX_CACHE.lock().get(&key).cloned()
}

fn dir_index_cache_invalidate(inode_num: u32, block_device: &Arc<dyn BlockDevice>) {
    let key = inode_cache_key(inode_num, block_device);
    DIR_INDEX_CACHE.lock().remove(&key);
}

fn extents_cached(
    inode_num: u32,
    block_device: &Arc<dyn BlockDevice>,
) -> Option<Arc<Vec<Ext4Extent>>> {
    let key = inode_cache_key(inode_num, block_device);
    EXTENTS_CACHE.lock().get(&key).cloned()
}

fn extents_cache_put(
    inode_num: u32,
    block_device: &Arc<dyn BlockDevice>,
    extents: Vec<Ext4Extent>,
) -> Arc<Vec<Ext4Extent>> {
    let mut cache = EXTENTS_CACHE.lock();
    if cache.len() >= EXTENTS_CACHE_MAX {
        cache.clear();
    }
    let entry = Arc::new(extents);
    let key = inode_cache_key(inode_num, block_device);
    cache.insert(key, Arc::clone(&entry));
    entry
}

fn extents_cache_invalidate(inode_num: u32, block_device: &Arc<dyn BlockDevice>) {
    let key = inode_cache_key(inode_num, block_device);
    EXTENTS_CACHE.lock().remove(&key);
}

fn inode_caches_invalidate(inode_num: u32, block_device: &Arc<dyn BlockDevice>) {
    dir_index_cache_invalidate(inode_num, block_device);
    extents_cache_invalidate(inode_num, block_device);
}

/// Virtual filesystem inode
pub struct Inode {
    /// Inode number
    inode_num: u32,
    /// Block containing the inode
    block_id: usize,
    /// Offset within the block
    block_offset: usize,
    /// Filesystem reference
    fs: Arc<Mutex<Ext4FileSystem>>,
    /// Block device reference
    block_device: Arc<dyn BlockDevice>,
    /// Cached block size (avoid locking fs repeatedly)
    block_size: usize,
}

impl Inode {
    pub fn inode_num(&self) -> u32 {
        self.inode_num
    }

    pub fn device_id(&self) -> usize {
        device_id(&self.block_device)
    }

    fn build_dir_index(&self) -> DirIndex {
        let mut index = DirIndex::new();
        if !self.is_dir() {
            return index;
        }

        let dir_size = self.size() as usize;
        let block_size = self.block_size;
        let mut buf = alloc::vec![0u8; block_size];

        let mut block_off = 0usize;
        while block_off < dir_size {
            let to_read = core::cmp::min(block_size, dir_size - block_off);
            if to_read < Ext4DirEntry::FIXED_SIZE {
                break;
            }
            self.read_at(block_off, &mut buf[..to_read]);

            let mut pos = 0usize;
            while pos + Ext4DirEntry::FIXED_SIZE <= to_read {
                let entry: &Ext4DirEntry =
                    unsafe { &*(buf.as_ptr().add(pos) as *const Ext4DirEntry) };
                if entry.rec_len == 0 {
                    break;
                }
                let rec_len = entry.rec_len as usize;
                if rec_len < Ext4DirEntry::FIXED_SIZE || pos + rec_len > to_read {
                    break;
                }
                if entry.inode != 0 && entry.name_len > 0 {
                    let name_off = pos + Ext4DirEntry::FIXED_SIZE;
                    let name_end = name_off + entry.name_len as usize;
                    if name_end <= pos + rec_len && name_end <= to_read {
                        let name_bytes = &buf[name_off..name_end];
                        if let Ok(name) = core::str::from_utf8(name_bytes) {
                            index.insert(String::from(name), (entry.inode, entry.file_type));
                        }
                    }
                }
                pos += rec_len;
            }
            block_off += block_size;
        }
        index
    }

    fn dir_index(&self) -> Arc<Mutex<DirIndex>> {
        if let Some(existing) = dir_index_cached(self.inode_num, &self.block_device) {
            return existing;
        }

        let built = self.build_dir_index();
        let entry = Arc::new(Mutex::new(built));
        let mut cache = DIR_INDEX_CACHE.lock();
        if cache.len() >= DIR_INDEX_CACHE_MAX {
            cache.clear();
        }
        let key = inode_cache_key(self.inode_num, &self.block_device);
        cache.entry(key).or_insert_with(|| Arc::clone(&entry));
        cache.get(&key).unwrap().clone()
    }

    /// Create a new inode reference (locks fs to get block_size)
    pub fn new(
        inode_num: u32,
        block_id: usize,
        block_offset: usize,
        fs: Arc<Mutex<Ext4FileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        let block_size = fs.lock().block_size();
        Self {
            inode_num,
            block_id,
            block_offset,
            fs,
            block_device,
            block_size,
        }
    }

    /// Create a new inode reference with pre-computed block_size (avoids locking)
    pub fn new_with_block_size(
        inode_num: u32,
        block_id: usize,
        block_offset: usize,
        fs: Arc<Mutex<Ext4FileSystem>>,
        block_device: Arc<dyn BlockDevice>,
        block_size: usize,
    ) -> Self {
        Self {
            inode_num,
            block_id,
            block_offset,
            fs,
            block_device,
            block_size,
        }
    }

    /// Get cached block size
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Read the disk inode
    fn read_disk_inode<V>(&self, f: impl FnOnce(&Ext4Inode) -> V) -> V {
        let cache = get_block_cache(self.block_id, Arc::clone(&self.block_device));
        let guard = cache.lock();
        guard.read(self.block_offset, f)
    }

    /// Modify the disk inode in-place.
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut Ext4Inode) -> V) -> V {
        let cache = get_block_cache(self.block_id, Arc::clone(&self.block_device));
        let mut guard = cache.lock();
        let v = guard.modify(self.block_offset, f);
        v
    }

    /// Check if this inode is a directory
    pub fn is_dir(&self) -> bool {
        self.read_disk_inode(|inode| inode.is_dir())
    }

    /// Check if this inode is a file
    pub fn is_file(&self) -> bool {
        self.read_disk_inode(|inode| inode.is_file())
    }

    /// Check if this inode is a symbolic link
    pub fn is_symlink(&self) -> bool {
        self.read_disk_inode(|inode| inode.is_symlink())
    }

    /// Check if this inode is a FIFO.
    pub fn is_fifo(&self) -> bool {
        self.read_disk_inode(|inode| inode.is_fifo())
    }

    /// Check if this inode is a character device.
    pub fn is_chrdev(&self) -> bool {
        self.read_disk_inode(|inode| inode.is_chrdev())
    }

    /// Check if this inode is a block device.
    pub fn is_blkdev(&self) -> bool {
        self.read_disk_inode(|inode| inode.is_blkdev())
    }

    /// Check if this inode is a socket.
    pub fn is_socket(&self) -> bool {
        self.read_disk_inode(|inode| inode.is_socket())
    }

    /// Get inode mode bits (file type + permissions).
    pub fn mode(&self) -> u16 {
        self.read_disk_inode(|inode| inode.i_mode)
    }

    /// Get raw device id for special files.
    pub fn special_rdev(&self) -> u64 {
        self.read_disk_inode(|inode| ((inode.i_block[1] as u64) << 32) | inode.i_block[0] as u64)
    }

    /// Get inode owner UID (low 16 bits).
    pub fn uid(&self) -> u32 {
        self.read_disk_inode(|inode| inode.i_uid as u32)
    }

    /// Get hard-link count.
    pub fn link_count(&self) -> u32 {
        self.read_disk_inode(|inode| inode.i_links_count as u32)
    }

    /// Get inode group GID (low 16 bits).
    pub fn gid(&self) -> u32 {
        self.read_disk_inode(|inode| inode.i_gid as u32)
    }

    /// Update inode owner UID/GID (low 16 bits).
    pub fn set_uid_gid(&self, uid: u32, gid: u32) {
        self.modify_disk_inode(|inode| {
            inode.i_uid = uid as u16;
            inode.i_gid = gid as u16;
        });
    }

    /// Update inode permission bits (and sticky/suid/sgid) while keeping file type.
    pub fn set_mode(&self, mode: u16) {
        self.modify_disk_inode(|inode| {
            let file_type = inode.i_mode & S_IFMT;
            inode.i_mode = file_type | (mode & 0o7777);
        });
    }

    /// Get file size
    pub fn size(&self) -> u64 {
        self.read_disk_inode(|inode| inode.size())
    }

    /// Find a file/directory by name in current directory
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        if !self.is_dir() {
            return None;
        }

        let idx = self.dir_index();
        let (inode_num, file_type) = { idx.lock().get(name).copied()? };

        let fs = self.fs.lock();
        let (block_id, offset) = fs.get_inode_pos(inode_num);
        let block_size = fs.block_size();
        drop(fs);

        let inode = Arc::new(Inode::new_with_block_size(
            inode_num,
            block_id,
            offset,
            Arc::clone(&self.fs),
            Arc::clone(&self.block_device),
            block_size,
        ));
        inode.repair_mode_if_missing(file_type);
        Some(inode)
    }

    fn repair_mode_if_missing(&self, file_type: u8) {
        let mode = self.mode();
        let current = mode & S_IFMT;
        if current != 0 {
            return;
        }
        let ftype = match file_type {
            Ext4DirEntry::FT_DIR => {
                if self.looks_like_dir() {
                    S_IFDIR
                } else {
                    S_IFREG
                }
            }
            Ext4DirEntry::FT_REG_FILE => S_IFREG,
            Ext4DirEntry::FT_SYMLINK => S_IFLNK,
            Ext4DirEntry::FT_FIFO => S_IFIFO,
            Ext4DirEntry::FT_CHRDEV => S_IFCHR,
            Ext4DirEntry::FT_BLKDEV => S_IFBLK,
            Ext4DirEntry::FT_SOCK => S_IFSOCK,
            _ => {
                if self.looks_like_dir() {
                    S_IFDIR
                } else {
                    S_IFREG
                }
            }
        };
        let default_perm = match ftype {
            S_IFDIR => 0o755,
            S_IFREG => 0o644,
            S_IFLNK => 0o777,
            S_IFIFO => 0o644,
            S_IFCHR => 0o600,
            S_IFBLK => 0o600,
            S_IFSOCK => 0o644,
            _ => 0o644,
        };
        let mut perm = mode & 0o777;
        if perm == 0 {
            perm = default_perm;
        }
        if ftype == S_IFDIR {
            perm |= 0o111;
        }
        self.modify_disk_inode(|inode| {
            inode.i_mode = ftype | perm;
        });
    }

    fn looks_like_dir(&self) -> bool {
        let size = self.size() as usize;
        if size < Ext4DirEntry::FIXED_SIZE * 2 {
            return false;
        }
        let to_read = core::cmp::min(self.block_size, size);
        let mut buf = alloc::vec![0u8; to_read];
        let read = self.read_at(0, &mut buf);
        if read < Ext4DirEntry::FIXED_SIZE * 2 {
            return false;
        }
        let (dot_inode, dot_rec, dot_len, _dot_type) = match Self::parse_dirent(&buf, 0) {
            Some(v) => v,
            None => return false,
        };
        if dot_inode != self.inode_num {
            return false;
        }
        if dot_len != 1 || buf.get(Ext4DirEntry::FIXED_SIZE) != Some(&b'.') {
            return false;
        }
        let next = dot_rec;
        if next >= read {
            return false;
        }
        let (dotdot_inode, _dotdot_rec, dotdot_len, _dotdot_type) =
            match Self::parse_dirent(&buf, next) {
                Some(v) => v,
                None => return false,
            };
        if dotdot_inode == 0 {
            return false;
        }
        if dotdot_len != 2 {
            return false;
        }
        let name_off = next + Ext4DirEntry::FIXED_SIZE;
        buf.get(name_off..name_off + 2) == Some(b"..")
    }

    fn parse_dirent(buf: &[u8], off: usize) -> Option<(u32, usize, usize, u8)> {
        if off + Ext4DirEntry::FIXED_SIZE > buf.len() {
            return None;
        }
        let inode = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
        let rec_len = u16::from_le_bytes([buf[off + 4], buf[off + 5]]) as usize;
        let name_len = buf[off + 6] as usize;
        let file_type = buf[off + 7];
        if rec_len < Ext4DirEntry::FIXED_SIZE || off + rec_len > buf.len() {
            return None;
        }
        Some((inode, rec_len, name_len, file_type))
    }

    /// List directory contents as (name, inode_num) pairs
    fn read_dir_entries(&self) -> Vec<(String, u32, u8)> {
        let mut entries = Vec::new();

        if !self.is_dir() {
            return entries;
        }

        let dir_size = self.size() as usize;
        let block_size = self.block_size;
        let mut buf = alloc::vec![0u8; block_size];

        let mut block_off = 0usize;
        while block_off < dir_size {
            let to_read = core::cmp::min(block_size, dir_size - block_off);
            if to_read < Ext4DirEntry::FIXED_SIZE {
                break;
            }
            self.read_at(block_off, &mut buf[..to_read]);

            let mut pos = 0usize;
            while pos + Ext4DirEntry::FIXED_SIZE <= to_read {
                let entry: &Ext4DirEntry =
                    unsafe { &*(buf.as_ptr().add(pos) as *const Ext4DirEntry) };
                if entry.rec_len == 0 {
                    break;
                }
                let rec_len = entry.rec_len as usize;
                if rec_len < Ext4DirEntry::FIXED_SIZE || pos + rec_len > to_read {
                    break;
                }
                if entry.inode != 0 && entry.name_len > 0 {
                    let name_start = pos + Ext4DirEntry::FIXED_SIZE;
                    let name_end = name_start + entry.name_len as usize;
                    if name_end <= pos + rec_len && name_end <= to_read {
                        let name_bytes = &buf[name_start..name_end];
                        if let Ok(name) = core::str::from_utf8(name_bytes) {
                            entries.push((String::from(name), entry.inode, entry.file_type));
                        }
                    }
                }
                pos += rec_len;
            }

            block_off += block_size;
        }

        entries
    }

    pub fn dir_entries(&self) -> Vec<(String, u32, u8)> {
        self.read_dir_entries()
    }

    /// List directory contents (names only)
    pub fn ls(&self) -> Vec<String> {
        self.read_dir_entries()
            .into_iter()
            .map(|(name, _, _)| name)
            .filter(|name| name != "." && name != "..")
            .collect()
    }

    /// Read data from inode at given offset
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let file_size = self.size() as usize;
        if offset >= file_size {
            return 0;
        }

        let read_len = buf.len().min(file_size - offset);
        if read_len == 0 {
            return 0;
        }

        // Fast symlink: inline target stored in i_block (no data blocks).
        let (is_symlink, block_data) = self.read_disk_inode(|inode| {
            let mut data = [0u8; 60];
            data.copy_from_slice(inode.block_data());
            (inode.is_symlink(), data)
        });
        if is_symlink && file_size <= block_data.len() {
            let end = offset.saturating_add(read_len).min(file_size);
            let len = end.saturating_sub(offset);
            if len > 0 {
                buf[..len].copy_from_slice(&block_data[offset..end]);
            }
            return len;
        }

        let uses_extents = self.read_disk_inode(|inode| inode.uses_extents());
        let block_size = self.block_size; // Use cached block size

        if uses_extents {
            self.read_extents(offset, &mut buf[..read_len], block_size)
        } else {
            // Ensure holes (sparse blocks) read back as zeros.
            buf[..read_len].fill(0);
            self.read_indirect(offset, &mut buf[..read_len], block_size)
        }
    }

    /// Read data using extent tree
    fn read_extents(&self, offset: usize, buf: &mut [u8], block_size: usize) -> usize {
        let extents = if let Some(cached) = extents_cached(self.inode_num, &self.block_device) {
            cached
        } else {
            let inode_data = self.read_disk_inode(|inode| {
                let mut data = [0u32; 15];
                data.copy_from_slice(&inode.i_block);
                data
            });
            let extents = self.parse_extent_tree(&inode_data, block_size);
            extents_cache_put(self.inode_num, &self.block_device, extents)
        };
        // Sparse regions inside an extent-based file must read back as zeros.
        buf.fill(0);
        self.read_from_extents(&extents, offset, buf, block_size)
    }

    /// Parse the extent tree from inode block data
    fn parse_extent_tree(&self, inode_block: &[u32; 15], block_size: usize) -> Vec<Ext4Extent> {
        let mut extents = Vec::new();

        // Read extent header
        let header_ptr = inode_block.as_ptr() as *const Ext4ExtentHeader;
        let header = unsafe { &*header_ptr };

        if !header.is_valid() {
            return extents;
        }

        if header.eh_depth == 0 {
            // Leaf node - extents are directly in i_block
            let extent_ptr = unsafe { (header_ptr as *const u8).add(12) as *const Ext4Extent };

            let entries = core::cmp::min(header.eh_entries as usize, EXTENTS_IN_INODE);
            for i in 0..entries {
                let extent = unsafe { &*extent_ptr.add(i) };
                extents.push(*extent);
            }
        } else {
            // Internal node - need to follow index entries
            self.parse_extent_tree_recursive(
                header,
                inode_block.as_ptr() as *const u8,
                block_size,
                &mut extents,
                EXTENTS_IN_INODE,
            );
        }

        extents
    }

    /// Recursively parse extent tree
    fn parse_extent_tree_recursive(
        &self,
        header: &Ext4ExtentHeader,
        node_ptr: *const u8,
        block_size: usize,
        extents: &mut Vec<Ext4Extent>,
        max_entries: usize,
    ) {
        if header.eh_depth == 0 {
            // Leaf node
            let extent_ptr = unsafe { node_ptr.add(12) as *const Ext4Extent };
            let entries = core::cmp::min(header.eh_entries as usize, max_entries);
            for i in 0..entries {
                let extent = unsafe { &*extent_ptr.add(i) };
                extents.push(*extent);
            }
        } else {
            // Internal node - follow index entries
            let idx_ptr = unsafe { node_ptr.add(12) as *const Ext4ExtentIdx };

            let entries = core::cmp::min(header.eh_entries as usize, max_entries);
            for i in 0..entries {
                let idx = unsafe { &*idx_ptr.add(i) };
                let child_block = idx.leaf_block() as usize;

                // Extent metadata is written through the block cache. Read it
                // back through the same cache so dirty child nodes are visible
                // before a full filesystem sync.
                let mut child_buf = alloc::vec![0u8; block_size];
                {
                    let cache = get_block_cache(child_block, Arc::clone(&self.block_device));
                    let guard = cache.lock();
                    let len = core::cmp::min(block_size, BLOCK_SZ);
                    child_buf[..len].copy_from_slice(guard.get_bytes(0, len));
                }

                let child_header = unsafe { &*(child_buf.as_ptr() as *const Ext4ExtentHeader) };
                if child_header.is_valid() {
                    self.parse_extent_tree_recursive(
                        child_header,
                        child_buf.as_ptr(),
                        block_size,
                        extents,
                        EXTENTS_PER_NODE,
                    );
                }
            }
        }
    }

    /// Read data from parsed extents
    fn read_from_extents(
        &self,
        extents: &[Ext4Extent],
        offset: usize,
        buf: &mut [u8],
        block_size: usize,
    ) -> usize {
        let file_start = offset;
        let file_end = offset.saturating_add(buf.len());

        for extent in extents {
            let extent_start = extent.ee_block as usize * block_size;
            let extent_len = extent.len() as usize * block_size;
            let extent_end = extent_start.saturating_add(extent_len);

            // No overlap.
            if extent_end <= file_start || extent_start >= file_end {
                continue;
            }

            let read_start = core::cmp::max(extent_start, file_start);
            let read_end = core::cmp::min(extent_end, file_end);
            if read_end <= read_start {
                continue;
            }

            // Destination offset in the caller's buffer (relative to requested `offset`).
            let mut dst_off = read_start - file_start;
            let mut remaining = read_end - read_start;

            // Calculate physical position within the extent.
            let offset_in_extent = read_start - extent_start;
            let mut cur_block = extent.start_block() as usize + offset_in_extent / block_size;
            let mut phys_off = offset_in_extent % block_size;

            while remaining > 0 {
                let cache = get_block_cache(cur_block, Arc::clone(&self.block_device));
                let to_read = remaining.min(block_size - phys_off);

                {
                    let guard = cache.lock();
                    let data = guard.get_bytes(phys_off, to_read);
                    buf[dst_off..dst_off + to_read].copy_from_slice(data);
                }

                dst_off += to_read;
                remaining -= to_read;
                phys_off = 0;
                cur_block += 1;
            }
        }

        buf.len()
    }

    /// Read data using indirect block pointers (legacy ext2/ext3 style)
    fn read_indirect(&self, offset: usize, buf: &mut [u8], block_size: usize) -> usize {
        let inode_blocks = self.read_disk_inode(|inode| inode.i_block);

        let ptrs_per_block = block_size / 4;
        let mut bytes_read = 0;
        let mut current_offset = offset;

        while bytes_read < buf.len() {
            let block_idx = current_offset / block_size;
            let offset_in_block = current_offset % block_size;

            let phys_block = self.get_block_num(&inode_blocks, block_idx, ptrs_per_block);
            if phys_block == 0 {
                break;
            }

            let cache = get_block_cache(phys_block as usize, Arc::clone(&self.block_device));
            let to_read = (buf.len() - bytes_read).min(block_size - offset_in_block);

            {
                let guard = cache.lock();
                let data = guard.get_bytes(offset_in_block, to_read);
                buf[bytes_read..bytes_read + to_read].copy_from_slice(data);
            }

            bytes_read += to_read;
            current_offset += to_read;
        }

        bytes_read
    }

    /// Get physical block number for logical block index
    fn get_block_num(&self, blocks: &[u32; 15], logical_idx: usize, ptrs_per_block: usize) -> u32 {
        // Direct blocks: 0-11
        if logical_idx < 12 {
            return blocks[logical_idx];
        }

        let logical_idx = logical_idx - 12;

        // Single indirect: 12
        if logical_idx < ptrs_per_block {
            if blocks[12] == 0 {
                return 0;
            }
            return self.read_indirect_block(blocks[12] as usize, logical_idx);
        }

        let logical_idx = logical_idx - ptrs_per_block;

        // Double indirect: 13
        if logical_idx < ptrs_per_block * ptrs_per_block {
            if blocks[13] == 0 {
                return 0;
            }
            let l1_idx = logical_idx / ptrs_per_block;
            let l2_idx = logical_idx % ptrs_per_block;
            let l1_block = self.read_indirect_block(blocks[13] as usize, l1_idx);
            if l1_block == 0 {
                return 0;
            }
            return self.read_indirect_block(l1_block as usize, l2_idx);
        }

        let logical_idx = logical_idx - ptrs_per_block * ptrs_per_block;

        // Triple indirect: 14
        if blocks[14] == 0 {
            return 0;
        }
        let l1_idx = logical_idx / (ptrs_per_block * ptrs_per_block);
        let remaining = logical_idx % (ptrs_per_block * ptrs_per_block);
        let l2_idx = remaining / ptrs_per_block;
        let l3_idx = remaining % ptrs_per_block;

        let l1_block = self.read_indirect_block(blocks[14] as usize, l1_idx);
        if l1_block == 0 {
            return 0;
        }
        let l2_block = self.read_indirect_block(l1_block as usize, l2_idx);
        if l2_block == 0 {
            return 0;
        }
        self.read_indirect_block(l2_block as usize, l3_idx)
    }

    /// Read a pointer from an indirect block
    fn read_indirect_block(&self, block_id: usize, index: usize) -> u32 {
        let cache = get_block_cache(block_id, Arc::clone(&self.block_device));
        let guard = cache.lock();
        guard.read(index * 4, |ptr: &u32| *ptr)
    }

    /// Read all data from inode
    pub fn read_all(&self) -> Vec<u8> {
        let size = self.size() as usize;
        let mut buf = alloc::vec![0u8; size];
        self.read_at(0, &mut buf);
        buf
    }

    /// Find an inode by a path relative to `self` (supports absolute paths too).
    pub fn find_path(&self, path: &str) -> Option<Arc<Inode>> {
        let mut cur: Arc<Inode> = if path.starts_with('/') {
            let root = Ext4FileSystem::root_inode(&self.fs);
            Arc::new(root)
        } else {
            Arc::new(Inode::new_with_block_size(
                self.inode_num,
                self.block_id,
                self.block_offset,
                Arc::clone(&self.fs),
                Arc::clone(&self.block_device),
                self.block_size,
            ))
        };

        for seg in path.split('/').filter(|s| !s.is_empty()) {
            if seg == "." {
                continue;
            }
            if let Some(next) = cur.find(seg) {
                cur = next;
            } else {
                return None;
            }
        }
        Some(cur)
    }

    fn read_extents_any(&self) -> Result<Vec<Ext4Extent>> {
        if !self.read_disk_inode(|inode| inode.uses_extents()) {
            return Err(Ext4Error::Unsupported);
        }
        let inode_data = self.read_disk_inode(|inode| {
            let mut data = [0u32; 15];
            data.copy_from_slice(&inode.i_block);
            data
        });
        let extents = self.parse_extent_tree(&inode_data, self.block_size);
        Ok(extents)
    }

    fn collect_extent_tree_block(
        block_device: &Arc<dyn BlockDevice>,
        pblock: u64,
        depth: u16,
        leaves: &mut Vec<u64>,
        indexes: &mut Vec<u64>,
    ) {
        if depth == 0 {
            leaves.push(pblock);
            return;
        }

        let child_blocks = {
            let cache = get_block_cache(pblock as usize, Arc::clone(block_device));
            let guard = cache.lock();
            let data = guard.get_bytes(0, BLOCK_SZ);
            let header = unsafe { &*(data.as_ptr() as *const Ext4ExtentHeader) };
            indexes.push(pblock);
            if !header.is_valid() {
                return;
            }

            let idx_ptr = unsafe { data.as_ptr().add(12) as *const Ext4ExtentIdx };
            let entries = core::cmp::min(header.eh_entries as usize, EXTENTS_PER_NODE);
            let mut child_blocks = Vec::new();
            for i in 0..entries {
                let idx = unsafe { &*idx_ptr.add(i) };
                child_blocks.push(idx.leaf_block());
            }
            child_blocks
        };

        for child in child_blocks {
            Self::collect_extent_tree_block(block_device, child, depth - 1, leaves, indexes);
        }
    }

    fn extent_tree_blocks(
        inode: &Ext4Inode,
        block_device: &Arc<dyn BlockDevice>,
    ) -> (Vec<u64>, Vec<u64>) {
        let header_ptr = inode.i_block.as_ptr() as *const Ext4ExtentHeader;
        let header = unsafe { &*header_ptr };
        if !header.is_valid() || header.eh_depth == 0 {
            return (Vec::new(), Vec::new());
        }
        let idx_ptr =
            unsafe { (inode.i_block.as_ptr() as *const u8).add(12) as *const Ext4ExtentIdx };
        let mut leaves = Vec::new();
        let mut indexes = Vec::new();
        for i in 0..core::cmp::min(header.eh_entries as usize, EXTENTS_IN_INODE) {
            let idx = unsafe { &*idx_ptr.add(i) };
            Self::collect_extent_tree_block(
                block_device,
                idx.leaf_block(),
                header.eh_depth - 1,
                &mut leaves,
                &mut indexes,
            );
        }
        (leaves, indexes)
    }

    fn write_extent_leaf_block(
        block_device: &Arc<dyn BlockDevice>,
        pblock: u64,
        extents: &[Ext4Extent],
    ) -> Result<()> {
        let cache = get_block_cache(pblock as usize, Arc::clone(block_device));
        let mut guard = cache.lock();
        guard.get_bytes_mut(0, BLOCK_SZ).fill(0);

        let header = Ext4ExtentHeader {
            eh_magic: Ext4ExtentHeader::MAGIC,
            eh_entries: extents.len() as u16,
            eh_max: EXTENTS_PER_NODE as u16,
            eh_depth: 0,
            eh_generation: 0,
        };
        guard.modify(0, |h: &mut Ext4ExtentHeader| *h = header);
        let extent_ptr =
            unsafe { (guard.get_bytes_mut(0, BLOCK_SZ).as_mut_ptr()).add(12) as *mut Ext4Extent };
        for (i, e) in extents.iter().enumerate() {
            unsafe { *extent_ptr.add(i) = *e };
        }
        Ok(())
    }

    fn write_extent_index_block(
        block_device: &Arc<dyn BlockDevice>,
        pblock: u64,
        entries: &[Ext4ExtentIdx],
        depth: u16,
    ) -> Result<()> {
        let cache = get_block_cache(pblock as usize, Arc::clone(block_device));
        let mut guard = cache.lock();
        guard.get_bytes_mut(0, BLOCK_SZ).fill(0);

        let header = Ext4ExtentHeader {
            eh_magic: Ext4ExtentHeader::MAGIC,
            eh_entries: entries.len() as u16,
            eh_max: EXTENTS_PER_NODE as u16,
            eh_depth: depth,
            eh_generation: 0,
        };
        guard.modify(0, |h: &mut Ext4ExtentHeader| *h = header);
        let idx_ptr = unsafe {
            (guard.get_bytes_mut(0, BLOCK_SZ).as_mut_ptr()).add(12) as *mut Ext4ExtentIdx
        };
        for (i, entry) in entries.iter().enumerate() {
            unsafe { *idx_ptr.add(i) = *entry };
        }
        Ok(())
    }

    fn write_extents_leaf_in_inode(inode: &mut Ext4Inode, extents: &[Ext4Extent]) -> Result<()> {
        if extents.len() > EXTENTS_IN_INODE {
            return Err(Ext4Error::Unsupported);
        }

        inode.i_flags |= EXT4_EXTENTS_FL;
        let header_ptr = inode.i_block.as_mut_ptr() as *mut Ext4ExtentHeader;
        let header = unsafe { &mut *header_ptr };
        header.eh_magic = Ext4ExtentHeader::MAGIC;
        header.eh_entries = extents.len() as u16;
        header.eh_max = EXTENTS_IN_INODE as u16;
        header.eh_depth = 0;
        header.eh_generation = 0;

        let extent_ptr =
            unsafe { (inode.i_block.as_mut_ptr() as *mut u8).add(12) as *mut Ext4Extent };
        for i in 0..EXTENTS_IN_INODE {
            let dst = unsafe { &mut *extent_ptr.add(i) };
            *dst = if i < extents.len() {
                extents[i]
            } else {
                Ext4Extent {
                    ee_block: 0,
                    ee_len: 0,
                    ee_start_hi: 0,
                    ee_start_lo: 0,
                }
            };
        }
        Ok(())
    }

    fn alloc_extent_metadata_blocks(fs: &mut Ext4FileSystem, count: usize) -> Result<Vec<u64>> {
        let mut blocks = Vec::new();
        for _ in 0..count {
            let Some(block) = fs.alloc_block() else {
                Self::dealloc_extent_metadata_blocks(fs, &blocks);
                return Err(Ext4Error::NoSpace);
            };
            blocks.push(block);
        }
        Ok(blocks)
    }

    fn dealloc_extent_metadata_blocks(fs: &mut Ext4FileSystem, blocks: &[u64]) {
        for &block in blocks {
            fs.dealloc_block(block);
        }
    }

    fn write_extent_root_index_in_inode(
        inode: &mut Ext4Inode,
        child_blocks: &[u64],
        extents: &[Ext4Extent],
        depth: u16,
        stride: usize,
    ) {
        let header_ptr = inode.i_block.as_mut_ptr() as *mut Ext4ExtentHeader;
        let header = unsafe { &mut *header_ptr };
        header.eh_magic = Ext4ExtentHeader::MAGIC;
        header.eh_entries = child_blocks.len() as u16;
        header.eh_max = EXTENTS_IN_INODE as u16;
        header.eh_depth = depth;
        header.eh_generation = 0;

        let idx_ptr =
            unsafe { (inode.i_block.as_mut_ptr() as *mut u8).add(12) as *mut Ext4ExtentIdx };
        for i in 0..EXTENTS_IN_INODE {
            let dst = unsafe { &mut *idx_ptr.add(i) };
            if i < child_blocks.len() {
                let start = i * stride;
                let ei_block = extents.get(start).map(|e| e.ee_block).unwrap_or(0);
                let child = child_blocks[i];
                *dst = Ext4ExtentIdx {
                    ei_block,
                    ei_leaf_lo: (child & 0xFFFF_FFFF) as u32,
                    ei_leaf_hi: (child >> 32) as u16,
                    ei_unused: 0,
                };
            } else {
                *dst = Ext4ExtentIdx {
                    ei_block: 0,
                    ei_leaf_lo: 0,
                    ei_leaf_hi: 0,
                    ei_unused: 0,
                };
            }
        }
    }

    fn write_extents_to_inode(
        &self,
        inode: &mut Ext4Inode,
        extents: &[Ext4Extent],
        fs: &mut Ext4FileSystem,
    ) -> Result<()> {
        inode.i_flags |= EXT4_EXTENTS_FL;
        let (old_leaf_blocks, old_index_blocks) =
            Self::extent_tree_blocks(inode, &self.block_device);

        // If the extents fit in the inode, store as a depth-0 leaf.
        if extents.len() <= EXTENTS_IN_INODE {
            let r = Self::write_extents_leaf_in_inode(inode, extents);
            if r.is_ok() {
                Self::dealloc_extent_metadata_blocks(fs, &old_leaf_blocks);
                Self::dealloc_extent_metadata_blocks(fs, &old_index_blocks);
            }
            extents_cache_invalidate(self.inode_num, &self.block_device);
            return r;
        }

        let need_leaf = (extents.len() + EXTENTS_PER_NODE - 1) / EXTENTS_PER_NODE;
        if need_leaf > EXTENTS_IN_INODE {
            let need_index = (need_leaf + EXTENTS_PER_NODE - 1) / EXTENTS_PER_NODE;
            if need_index > EXTENTS_IN_INODE {
                return Err(Ext4Error::Unsupported);
            }
        }

        let leaf_blocks = Self::alloc_extent_metadata_blocks(fs, need_leaf)?;

        for (i, pblock) in leaf_blocks.iter().copied().enumerate() {
            let start = i * EXTENTS_PER_NODE;
            let end = core::cmp::min(extents.len(), start + EXTENTS_PER_NODE);
            if let Err(e) =
                Self::write_extent_leaf_block(&self.block_device, pblock, &extents[start..end])
            {
                Self::dealloc_extent_metadata_blocks(fs, &leaf_blocks);
                return Err(e);
            }
        }

        // Depth-1: the inode root points directly to leaf blocks.
        if need_leaf <= EXTENTS_IN_INODE {
            Self::write_extent_root_index_in_inode(
                inode,
                &leaf_blocks,
                extents,
                1,
                EXTENTS_PER_NODE,
            );
            Self::dealloc_extent_metadata_blocks(fs, &old_leaf_blocks);
            Self::dealloc_extent_metadata_blocks(fs, &old_index_blocks);
            extents_cache_invalidate(self.inode_num, &self.block_device);
            return Ok(());
        }

        // Depth-2: the inode root points to index blocks, which point to leaf blocks.
        let need_index = (need_leaf + EXTENTS_PER_NODE - 1) / EXTENTS_PER_NODE;
        let index_blocks = match Self::alloc_extent_metadata_blocks(fs, need_index) {
            Ok(blocks) => blocks,
            Err(e) => {
                Self::dealloc_extent_metadata_blocks(fs, &leaf_blocks);
                return Err(e);
            }
        };

        for (i, pblock) in index_blocks.iter().copied().enumerate() {
            let leaf_start = i * EXTENTS_PER_NODE;
            let leaf_end = core::cmp::min(need_leaf, leaf_start + EXTENTS_PER_NODE);
            let mut entries = Vec::new();
            for leaf_idx in leaf_start..leaf_end {
                let extent_idx = leaf_idx * EXTENTS_PER_NODE;
                let ei_block = extents.get(extent_idx).map(|e| e.ee_block).unwrap_or(0);
                let leaf = leaf_blocks[leaf_idx];
                entries.push(Ext4ExtentIdx {
                    ei_block,
                    ei_leaf_lo: (leaf & 0xFFFF_FFFF) as u32,
                    ei_leaf_hi: (leaf >> 32) as u16,
                    ei_unused: 0,
                });
            }
            if let Err(e) = Self::write_extent_index_block(&self.block_device, pblock, &entries, 1)
            {
                Self::dealloc_extent_metadata_blocks(fs, &leaf_blocks);
                Self::dealloc_extent_metadata_blocks(fs, &index_blocks);
                return Err(e);
            }
        }

        Self::write_extent_root_index_in_inode(
            inode,
            &index_blocks,
            extents,
            2,
            EXTENTS_PER_NODE * EXTENTS_PER_NODE,
        );
        Self::dealloc_extent_metadata_blocks(fs, &old_leaf_blocks);
        Self::dealloc_extent_metadata_blocks(fs, &old_index_blocks);
        extents_cache_invalidate(self.inode_num, &self.block_device);
        Ok(())
    }

    fn dir_entry_file_type(&self) -> u8 {
        Self::dir_entry_file_type_from_mode(self.mode())
    }

    fn dir_entry_file_type_from_mode(mode: u16) -> u8 {
        match mode & S_IFMT {
            S_IFREG => Ext4DirEntry::FT_REG_FILE,
            S_IFDIR => Ext4DirEntry::FT_DIR,
            S_IFCHR => Ext4DirEntry::FT_CHRDEV,
            S_IFBLK => Ext4DirEntry::FT_BLKDEV,
            S_IFIFO => Ext4DirEntry::FT_FIFO,
            S_IFSOCK => Ext4DirEntry::FT_SOCK,
            S_IFLNK => Ext4DirEntry::FT_SYMLINK,
            _ => Ext4DirEntry::FT_UNKNOWN,
        }
    }

    fn extents_blocks_count(extents: &[Ext4Extent]) -> u32 {
        extents.iter().map(|e| e.len()).sum()
    }

    fn sectors_per_block(&self) -> u32 {
        (self.block_size as u32) / 512
    }

    fn map_logical_block(extents: &[Ext4Extent], lblock: u32) -> Option<u64> {
        for e in extents {
            let start = e.ee_block;
            let end = start.saturating_add(e.len());
            if lblock >= start && lblock < end {
                let off = (lblock - start) as u64;
                return Some(e.start_block().saturating_add(off));
            }
        }
        None
    }

    fn push_block_to_extents(
        extents: &mut Vec<Ext4Extent>,
        lblock: u32,
        pblock: u64,
    ) -> Result<()> {
        if let Some(last) = extents.last_mut() {
            let last_lend = last.ee_block.saturating_add(last.len());
            let last_pend = last.start_block().saturating_add(last.len() as u64);
            if lblock == last_lend && pblock == last_pend && last.ee_len < 0x7FFF {
                last.ee_len += 1;
                return Ok(());
            }
        }
        extents.push(Ext4Extent {
            ee_block: lblock,
            ee_len: 1,
            ee_start_hi: (pblock >> 32) as u16,
            ee_start_lo: (pblock & 0xFFFF_FFFF) as u32,
        });
        Ok(())
    }

    fn insert_block_mapping(extents: &mut Vec<Ext4Extent>, lblock: u32, pblock: u64) -> Result<()> {
        let mut insert_pos = extents.len();
        for (idx, extent) in extents.iter().enumerate() {
            let start = extent.ee_block;
            let end = start.saturating_add(extent.len());
            if lblock < start {
                insert_pos = idx;
                break;
            }
            if lblock >= start && lblock < end {
                return Ok(());
            }
        }

        let mut merge_prev = false;
        if insert_pos > 0 {
            let prev = &extents[insert_pos - 1];
            let prev_lend = prev.ee_block.saturating_add(prev.len());
            let prev_pend = prev.start_block().saturating_add(prev.len() as u64);
            if lblock == prev_lend && pblock == prev_pend && prev.ee_len < 0x7FFF {
                merge_prev = true;
            }
        }

        let mut merge_next = false;
        if insert_pos < extents.len() {
            let next = &extents[insert_pos];
            if lblock.saturating_add(1) == next.ee_block
                && pblock.saturating_add(1) == next.start_block()
                && next.ee_len < 0x7FFF
            {
                merge_next = true;
            }
        }

        if merge_prev && merge_next {
            let prev_len = extents[insert_pos - 1].ee_len as u32;
            let next_len = extents[insert_pos].ee_len as u32;
            let merged_len = prev_len.saturating_add(1).saturating_add(next_len);
            if merged_len <= 0x7FFF {
                extents[insert_pos - 1].ee_len = merged_len as u16;
                extents.remove(insert_pos);
                return Ok(());
            }
            merge_next = false;
        }

        if merge_prev {
            extents[insert_pos - 1].ee_len += 1;
            return Ok(());
        }

        if merge_next {
            let next = &mut extents[insert_pos];
            next.ee_block = lblock;
            next.ee_start_hi = (pblock >> 32) as u16;
            next.ee_start_lo = (pblock & 0xFFFF_FFFF) as u32;
            next.ee_len += 1;
            return Ok(());
        }

        extents.insert(
            insert_pos,
            Ext4Extent {
                ee_block: lblock,
                ee_len: 1,
                ee_start_hi: (pblock >> 32) as u16,
                ee_start_lo: (pblock & 0xFFFF_FFFF) as u32,
            },
        );
        Ok(())
    }

    fn ensure_write_blocks(&self, start_lblock: u32, end_lblock: u32) -> Result<Vec<Ext4Extent>> {
        let mut extents = if let Some(cached) = extents_cached(self.inode_num, &self.block_device) {
            cached.as_ref().clone()
        } else {
            self.read_extents_any().unwrap_or_default()
        };
        if start_lblock >= end_lblock {
            extents_cache_put(self.inode_num, &self.block_device, extents.clone());
            return Ok(extents);
        }

        let mut changed = false;
        {
            let mut fs = self.fs.lock();
            for lblock in start_lblock..end_lblock {
                if Self::map_logical_block(&extents, lblock).is_some() {
                    continue;
                }
                let pblock = fs.alloc_block().ok_or(Ext4Error::NoSpace)?;
                {
                    let cache = super::block_cache::get_block_cache_zeroed(
                        pblock as usize,
                        Arc::clone(&self.block_device),
                    );
                    let mut guard = cache.lock();
                    guard.get_bytes_mut(0, BLOCK_SZ).fill(0);
                }
                Self::insert_block_mapping(&mut extents, lblock, pblock)?;
                changed = true;
            }

            if changed {
                let blocks = Self::extents_blocks_count(&extents);
                let mut inode = self.read_disk_inode(|i| *i);
                inode.i_blocks_lo = blocks.saturating_mul(self.sectors_per_block());
                self.write_extents_to_inode(&mut inode, &extents, &mut *fs)?;
                fs.write_inode(self.inode_num, &inode);
            }
        }

        extents_cache_put(self.inode_num, &self.block_device, extents.clone());
        Ok(extents)
    }

    fn ensure_blocks(&self, total_blocks: u32) -> Result<Vec<Ext4Extent>> {
        let mut extents = if let Some(cached) = extents_cached(self.inode_num, &self.block_device) {
            cached.as_ref().clone()
        } else {
            self.read_extents_any().unwrap_or_default()
        };
        let cur_blocks = extents
            .iter()
            .map(|e| e.ee_block.saturating_add(e.len()))
            .max()
            .unwrap_or(0);

        if total_blocks <= cur_blocks {
            extents_cache_put(self.inode_num, &self.block_device, extents.clone());
            return Ok(extents);
        }

        let mut fs = self.fs.lock();
        for lblock in cur_blocks..total_blocks {
            let pblock = fs.alloc_block().ok_or(Ext4Error::NoSpace)?;
            {
                let cache = super::block_cache::get_block_cache_zeroed(
                    pblock as usize,
                    Arc::clone(&self.block_device),
                );
                let mut guard = cache.lock();
                guard.get_bytes_mut(0, BLOCK_SZ).fill(0);
            }
            Self::push_block_to_extents(&mut extents, lblock, pblock)?;
        }
        drop(fs);

        // Persist updated extents and inode metadata.
        let blocks = Self::extents_blocks_count(&extents);
        let mut inode = self.read_disk_inode(|i| *i);
        inode.i_blocks_lo = blocks.saturating_mul(self.sectors_per_block());
        {
            let mut fs = self.fs.lock();
            self.write_extents_to_inode(&mut inode, &extents, &mut *fs)?;
            fs.write_inode(self.inode_num, &inode);
        }

        extents_cache_put(self.inode_num, &self.block_device, extents.clone());
        Ok(extents)
    }

    /// Write data to inode at given offset.
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> {
        let is_symlink = self.read_disk_inode(|inode| inode.is_symlink());
        if !self.is_file() && !is_symlink {
            return Err(Ext4Error::NotAFile);
        }
        if buf.is_empty() {
            return Ok(0);
        }

        if is_symlink {
            let end = offset.saturating_add(buf.len());
            if end <= 60 {
                self.modify_disk_inode(|inode| {
                    let data = inode.block_data_mut();
                    data.fill(0);
                    data[offset..end].copy_from_slice(buf);
                    inode.i_size_lo = (end as u64 & 0xFFFF_FFFF) as u32;
                    inode.i_size_high = ((end as u64 >> 32) & 0xFFFF_FFFF) as u32;
                });
                return Ok(buf.len());
            }
        }

        let old_size = self.size() as usize;
        let end = offset.saturating_add(buf.len());
        let new_size = max(old_size, end);
        let block_size = self.block_size;
        let start_lblock = (offset / block_size) as u32;
        let end_lblock = ((end.saturating_add(block_size - 1)) / block_size) as u32;

        let extents = self.ensure_write_blocks(start_lblock, end_lblock)?;

        let mut written = 0usize;
        let mut cur_off = offset;
        while written < buf.len() {
            let lblock = (cur_off / block_size) as u32;
            let off_in_block = cur_off % block_size;
            let to_write = (buf.len() - written).min(block_size - off_in_block);
            let pblock = Self::map_logical_block(&extents, lblock).ok_or(Ext4Error::Unsupported)?;

            let cache = get_block_cache(pblock as usize, Arc::clone(&self.block_device));
            let mut guard = cache.lock();
            guard
                .get_bytes_mut(off_in_block, to_write)
                .copy_from_slice(&buf[written..written + to_write]);

            written += to_write;
            cur_off += to_write;
        }

        if new_size != old_size {
            self.modify_disk_inode(|inode| {
                inode.i_size_lo = (new_size as u64 & 0xFFFF_FFFF) as u32;
                inode.i_size_high = ((new_size as u64 >> 32) & 0xFFFF_FFFF) as u32;
            });
        }

        Ok(written)
    }

    /// Truncate a regular file to size 0 (frees all data blocks).
    pub fn clear(&self) -> Result<()> {
        if !self.is_file() {
            return Err(Ext4Error::NotAFile);
        }
        self.free_all_blocks()?;

        self.modify_disk_inode(|inode| {
            inode.i_size_lo = 0;
            inode.i_size_high = 0;
            inode.i_blocks_lo = 0;
            // Reset extent tree to empty leaf.
            let header_ptr = inode.i_block.as_mut_ptr() as *mut Ext4ExtentHeader;
            let header = unsafe { &mut *header_ptr };
            header.eh_magic = Ext4ExtentHeader::MAGIC;
            header.eh_entries = 0;
            header.eh_max = ((15 * 4 - 12) / 12) as u16;
            header.eh_depth = 0;
            header.eh_generation = 0;
        });

        extents_cache_invalidate(self.inode_num, &self.block_device);
        Ok(())
    }

    fn free_all_blocks(&self) -> Result<()> {
        let extents = self.read_extents_any().unwrap_or_default();
        let inode = self.read_disk_inode(|i| *i);
        let mut fs = self.fs.lock();
        for e in &extents {
            let start = e.start_block();
            for b in 0..e.len() as u64 {
                fs.dealloc_block(start + b);
            }
        }
        // Also free extent-tree metadata blocks for depth-1/depth-2 trees.
        let (leaf_blocks, index_blocks) = Self::extent_tree_blocks(&inode, &self.block_device);
        for b in leaf_blocks {
            fs.dealloc_block(b);
        }
        for b in index_blocks {
            fs.dealloc_block(b);
        }
        Ok(())
    }

    fn dir_write_entry_at(
        &self,
        pblock: u64,
        entry_off: usize,
        rec_len: u16,
        inode_num: u32,
        file_type: u8,
        name: &str,
    ) -> Result<()> {
        if name.len() > EXT4_NAME_LEN {
            return Err(Ext4Error::NameTooLong);
        }
        let need = align4(Ext4DirEntry::FIXED_SIZE + name.len());
        if need > rec_len as usize {
            return Err(Ext4Error::NoSpace);
        }

        let cache = get_block_cache(pblock as usize, Arc::clone(&self.block_device));
        let mut guard = cache.lock();

        guard.modify(entry_off, |e: &mut Ext4DirEntry| {
            e.inode = inode_num;
            e.rec_len = rec_len;
            e.name_len = name.len() as u8;
            e.file_type = file_type;
        });

        let name_off = entry_off + Ext4DirEntry::FIXED_SIZE;
        guard
            .get_bytes_mut(name_off, name.len())
            .copy_from_slice(name.as_bytes());

        let rest = rec_len as usize - Ext4DirEntry::FIXED_SIZE - name.len();
        if rest > 0 {
            guard.get_bytes_mut(name_off + name.len(), rest).fill(0);
        }

        Ok(())
    }

    fn dir_add_entry(&self, name: &str, inode_num: u32, file_type: u8) -> Result<()> {
        if !self.is_dir() {
            return Err(Ext4Error::NotADirectory);
        }
        if name.is_empty() {
            return Err(Ext4Error::InvalidInput);
        }
        if self.find(name).is_some() {
            return Err(Ext4Error::AlreadyExists);
        }

        let need = align4(Ext4DirEntry::FIXED_SIZE + name.len());
        if need > self.block_size {
            return Err(Ext4Error::NameTooLong);
        }

        let dir_size = self.size() as usize;
        let block_size = self.block_size;
        let blocks = if dir_size == 0 {
            0
        } else {
            (dir_size + block_size - 1) / block_size
        };

        let extents = self.ensure_blocks(blocks as u32)?;

        for lblock in 0..blocks {
            let pblock =
                Self::map_logical_block(&extents, lblock as u32).ok_or(Ext4Error::Unsupported)?;
            let cache = get_block_cache(pblock as usize, Arc::clone(&self.block_device));
            let guard = cache.lock();

            let mut off = 0usize;
            while off + Ext4DirEntry::FIXED_SIZE <= block_size {
                let entry = guard.get_ref::<Ext4DirEntry>(off);
                if entry.rec_len == 0 {
                    break;
                }
                let rec_len = entry.rec_len as usize;

                if entry.inode == 0 && rec_len >= need {
                    drop(guard);
                    return self.dir_write_entry_at(
                        pblock,
                        off,
                        rec_len as u16,
                        inode_num,
                        file_type,
                        name,
                    );
                }

                let used = align4(Ext4DirEntry::FIXED_SIZE + entry.name_len as usize);
                if rec_len >= used + need {
                    drop(guard);
                    let old_rec = rec_len as u16;
                    let new_off = off + used;
                    {
                        let cache =
                            get_block_cache(pblock as usize, Arc::clone(&self.block_device));
                        let mut g = cache.lock();
                        g.modify(off, |e: &mut Ext4DirEntry| {
                            e.rec_len = used as u16;
                        });
                    }
                    return self.dir_write_entry_at(
                        pblock,
                        new_off,
                        old_rec - used as u16,
                        inode_num,
                        file_type,
                        name,
                    );
                }

                off += rec_len;
                if off >= block_size {
                    break;
                }
            }
        }

        // Need a new block.
        let new_block_index = blocks as u32;
        let extents = self.ensure_blocks(new_block_index + 1)?;
        let pblock =
            Self::map_logical_block(&extents, new_block_index).ok_or(Ext4Error::Unsupported)?;

        self.dir_write_entry_at(pblock, 0, block_size as u16, inode_num, file_type, name)?;

        self.modify_disk_inode(|inode| {
            let new_size = (new_block_index as usize + 1) * block_size;
            inode.i_size_lo = (new_size as u64 & 0xFFFF_FFFF) as u32;
            inode.i_size_high = ((new_size as u64 >> 32) & 0xFFFF_FFFF) as u32;
        });

        Ok(())
    }

    /// Create a new empty regular file under this directory.
    pub fn create_file(&self, name: &str) -> Result<Arc<Inode>> {
        if !self.is_dir() {
            return Err(Ext4Error::NotADirectory);
        }
        if name.is_empty() {
            return Err(Ext4Error::InvalidInput);
        }
        if name.len() > EXT4_NAME_LEN {
            return Err(Ext4Error::NameTooLong);
        }
        if self.find(name).is_some() {
            return Err(Ext4Error::AlreadyExists);
        }

        let (inode_num, block_id, offset, block_size) = {
            let mut fs = self.fs.lock();
            let inode_num = fs.alloc_inode().ok_or(Ext4Error::NoSpace)?;
            inode_caches_invalidate(inode_num, &self.block_device);
            let (block_id, offset) = fs.get_inode_pos(inode_num);
            let block_size = fs.block_size();

            let mut inode: Ext4Inode = unsafe { core::mem::zeroed() };
            inode.i_mode = S_IFREG | 0o666;
            inode.i_links_count = 1;
            inode.i_flags = EXT4_EXTENTS_FL;
            let _ = Self::write_extents_leaf_in_inode(&mut inode, &[]);
            fs.write_inode(inode_num, &inode);

            (inode_num, block_id, offset, block_size)
        };

        if let Err(e) = self.dir_add_entry(name, inode_num, Ext4DirEntry::FT_REG_FILE) {
            let mut fs = self.fs.lock();
            fs.dealloc_inode(inode_num);
            return Err(e);
        }

        if let Some(idx) = dir_index_cached(self.inode_num, &self.block_device) {
            idx.lock()
                .insert(String::from(name), (inode_num, Ext4DirEntry::FT_REG_FILE));
        }

        Ok(Arc::new(Inode::new_with_block_size(
            inode_num,
            block_id,
            offset,
            Arc::clone(&self.fs),
            Arc::clone(&self.block_device),
            block_size,
        )))
    }

    /// Create a new special inode (fifo/chr/blk/socket) under this directory.
    pub fn create_special(&self, name: &str, mode: u16, rdev: u64) -> Result<Arc<Inode>> {
        if !self.is_dir() {
            return Err(Ext4Error::NotADirectory);
        }
        if name.is_empty() {
            return Err(Ext4Error::InvalidInput);
        }
        if name.len() > EXT4_NAME_LEN {
            return Err(Ext4Error::NameTooLong);
        }
        if self.find(name).is_some() {
            return Err(Ext4Error::AlreadyExists);
        }

        let ftype = Self::dir_entry_file_type_from_mode(mode);
        if !matches!(
            ftype,
            Ext4DirEntry::FT_FIFO
                | Ext4DirEntry::FT_CHRDEV
                | Ext4DirEntry::FT_BLKDEV
                | Ext4DirEntry::FT_SOCK
        ) {
            return Err(Ext4Error::InvalidInput);
        }

        let (inode_num, block_id, offset, block_size) = {
            let mut fs = self.fs.lock();
            let inode_num = fs.alloc_inode().ok_or(Ext4Error::NoSpace)?;
            inode_caches_invalidate(inode_num, &self.block_device);
            let (block_id, offset) = fs.get_inode_pos(inode_num);
            let block_size = fs.block_size();

            let mut inode: Ext4Inode = unsafe { core::mem::zeroed() };
            inode.i_mode = mode;
            inode.i_links_count = 1;
            inode.i_flags = 0;
            inode.i_block[0] = rdev as u32;
            inode.i_block[1] = (rdev >> 32) as u32;
            fs.write_inode(inode_num, &inode);

            (inode_num, block_id, offset, block_size)
        };

        if let Err(e) = self.dir_add_entry(name, inode_num, ftype) {
            let mut fs = self.fs.lock();
            fs.dealloc_inode(inode_num);
            return Err(e);
        }

        if let Some(idx) = dir_index_cached(self.inode_num, &self.block_device) {
            idx.lock().insert(String::from(name), (inode_num, ftype));
        }

        Ok(Arc::new(Inode::new_with_block_size(
            inode_num,
            block_id,
            offset,
            Arc::clone(&self.fs),
            Arc::clone(&self.block_device),
            block_size,
        )))
    }

    /// Create a new symbolic link under this directory.
    pub fn create_symlink(&self, name: &str, target: &str) -> Result<Arc<Inode>> {
        if !self.is_dir() {
            return Err(Ext4Error::NotADirectory);
        }
        if name.is_empty() {
            return Err(Ext4Error::InvalidInput);
        }
        if name.len() > EXT4_NAME_LEN {
            return Err(Ext4Error::NameTooLong);
        }
        if self.find(name).is_some() {
            return Err(Ext4Error::AlreadyExists);
        }

        let (inode_num, block_id, offset, block_size) = {
            let mut fs = self.fs.lock();
            let inode_num = fs.alloc_inode().ok_or(Ext4Error::NoSpace)?;
            inode_caches_invalidate(inode_num, &self.block_device);
            let (block_id, offset) = fs.get_inode_pos(inode_num);
            let block_size = fs.block_size();

            let mut inode: Ext4Inode = unsafe { core::mem::zeroed() };
            inode.i_mode = S_IFLNK | 0o777;
            inode.i_links_count = 1;
            inode.i_flags = EXT4_EXTENTS_FL;
            let _ = Self::write_extents_leaf_in_inode(&mut inode, &[]);
            fs.write_inode(inode_num, &inode);

            (inode_num, block_id, offset, block_size)
        };

        if let Err(e) = self.dir_add_entry(name, inode_num, Ext4DirEntry::FT_SYMLINK) {
            let mut fs = self.fs.lock();
            fs.dealloc_inode(inode_num);
            return Err(e);
        }

        if let Some(idx) = dir_index_cached(self.inode_num, &self.block_device) {
            idx.lock()
                .insert(String::from(name), (inode_num, Ext4DirEntry::FT_SYMLINK));
        }

        let inode = Arc::new(Inode::new_with_block_size(
            inode_num,
            block_id,
            offset,
            Arc::clone(&self.fs),
            Arc::clone(&self.block_device),
            block_size,
        ));

        let _ = inode.write_at(0, target.as_bytes());
        Ok(inode)
    }

    /// Create a hard-link to `target` in this directory.
    pub fn link_inode(&self, name: &str, target: &Arc<Inode>) -> Result<()> {
        if !self.is_dir() {
            return Err(Ext4Error::NotADirectory);
        }
        if name.is_empty() {
            return Err(Ext4Error::InvalidInput);
        }
        if name.len() > EXT4_NAME_LEN {
            return Err(Ext4Error::NameTooLong);
        }
        if self.find(name).is_some() {
            return Err(Ext4Error::AlreadyExists);
        }
        if target.is_dir() {
            return Err(Ext4Error::Unsupported);
        }

        self.dir_add_entry(name, target.inode_num(), target.dir_entry_file_type())?;
        target.modify_disk_inode(|inode| {
            inode.i_links_count = inode.i_links_count.saturating_add(1);
        });

        if let Some(idx) = dir_index_cached(self.inode_num, &self.block_device) {
            idx.lock().insert(
                String::from(name),
                (target.inode_num(), target.dir_entry_file_type()),
            );
        }
        Ok(())
    }

    /// Create a new directory under this directory.
    pub fn create_dir(&self, name: &str) -> Result<Arc<Inode>> {
        if !self.is_dir() {
            return Err(Ext4Error::NotADirectory);
        }
        if name.is_empty() {
            return Err(Ext4Error::InvalidInput);
        }
        if name.len() > EXT4_NAME_LEN {
            return Err(Ext4Error::NameTooLong);
        }
        if self.find(name).is_some() {
            return Err(Ext4Error::AlreadyExists);
        }

        let (inode_num, block_id, offset, block_size, data_block) = {
            let mut fs = self.fs.lock();
            let inode_num = fs.alloc_inode().ok_or(Ext4Error::NoSpace)?;
            inode_caches_invalidate(inode_num, &self.block_device);
            let data_block = fs.alloc_block().ok_or(Ext4Error::NoSpace)?;
            let (block_id, offset) = fs.get_inode_pos(inode_num);
            let block_size = fs.block_size();

            // Init directory block with "." and ".."
            {
                let cache = get_block_cache(data_block as usize, Arc::clone(&self.block_device));
                let mut guard = cache.lock();
                guard.get_bytes_mut(0, BLOCK_SZ).fill(0);
                guard.sync();
            }
            self.dir_write_entry_at(data_block, 0, 12, inode_num, Ext4DirEntry::FT_DIR, ".")?;
            self.dir_write_entry_at(
                data_block,
                12,
                (block_size - 12) as u16,
                self.inode_num,
                Ext4DirEntry::FT_DIR,
                "..",
            )?;

            let mut inode: Ext4Inode = unsafe { core::mem::zeroed() };
            inode.i_mode = S_IFDIR | 0o755;
            inode.i_links_count = 2;
            inode.i_flags = EXT4_EXTENTS_FL;
            inode.i_size_lo = block_size as u32;
            inode.i_blocks_lo = self.sectors_per_block();
            let extent = Ext4Extent {
                ee_block: 0,
                ee_len: 1,
                ee_start_hi: (data_block >> 32) as u16,
                ee_start_lo: (data_block & 0xFFFF_FFFF) as u32,
            };
            let _ = Self::write_extents_leaf_in_inode(&mut inode, &[extent]);
            fs.write_inode(inode_num, &inode);

            (inode_num, block_id, offset, block_size, data_block)
        };

        if let Err(e) = self.dir_add_entry(name, inode_num, Ext4DirEntry::FT_DIR) {
            let mut fs = self.fs.lock();
            fs.dealloc_block(data_block);
            fs.dealloc_inode(inode_num);
            return Err(e);
        }

        if let Some(idx) = dir_index_cached(self.inode_num, &self.block_device) {
            idx.lock()
                .insert(String::from(name), (inode_num, Ext4DirEntry::FT_DIR));
        }
        self.modify_disk_inode(|inode| {
            inode.i_links_count = inode.i_links_count.saturating_add(1);
        });

        Ok(Arc::new(Inode::new_with_block_size(
            inode_num,
            block_id,
            offset,
            Arc::clone(&self.fs),
            Arc::clone(&self.block_device),
            block_size,
        )))
    }

    fn dir_remove_entry(&self, name: &str) -> Result<u32> {
        if !self.is_dir() {
            return Err(Ext4Error::NotADirectory);
        }
        let dir_size = self.size() as usize;
        let block_size = self.block_size;
        let blocks = if dir_size == 0 {
            0
        } else {
            (dir_size + block_size - 1) / block_size
        };

        let extents = self.read_extents_any().unwrap_or_default();
        for lblock in 0..blocks {
            let Some(pblock) = Self::map_logical_block(&extents, lblock as u32) else {
                continue;
            };

            let cache = get_block_cache(pblock as usize, Arc::clone(&self.block_device));
            let guard = cache.lock();
            let mut off = 0usize;

            while off + Ext4DirEntry::FIXED_SIZE <= block_size {
                let entry = guard.get_ref::<Ext4DirEntry>(off);
                if entry.rec_len == 0 {
                    break;
                }

                let inode_num = entry.inode;
                if inode_num != 0 && entry.name_len > 0 {
                    let name_off = off + Ext4DirEntry::FIXED_SIZE;
                    let name_bytes = guard.get_bytes(name_off, entry.name_len as usize);
                    if name_bytes == name.as_bytes() {
                        drop(guard);
                        let cache =
                            get_block_cache(pblock as usize, Arc::clone(&self.block_device));
                        let mut g = cache.lock();
                        g.modify(off, |e: &mut Ext4DirEntry| {
                            e.inode = 0;
                        });
                        return Ok(inode_num);
                    }
                }

                off += entry.rec_len as usize;
                if off >= block_size {
                    break;
                }
            }
        }

        Err(Ext4Error::NotFound)
    }

    /// Remove a file or an empty directory from this directory.
    pub fn unlink(&self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(Ext4Error::InvalidInput);
        }
        if name == "." || name == ".." {
            return Err(Ext4Error::InvalidInput);
        }

        if !self.is_dir() {
            return Err(Ext4Error::NotADirectory);
        }

        let child = self.find(name).ok_or(Ext4Error::NotFound)?;
        let inode_num = child.inode_num();

        // If directory, require it to be empty (excluding "." and "..").
        let is_dir = child.is_dir();
        if is_dir {
            if !child.ls().is_empty() {
                return Err(Ext4Error::Unsupported);
            }
        }

        // Remove directory entry.
        let _ = self.dir_remove_entry(name)?;

        if is_dir {
            // rmdir: parent loses one link for child "..".
            self.modify_disk_inode(|inode| {
                inode.i_links_count = inode.i_links_count.saturating_sub(1);
            });
            child.free_all_blocks()?;
            child.modify_disk_inode(|inode| {
                inode.i_size_lo = 0;
                inode.i_size_high = 0;
                inode.i_blocks_lo = 0;
                inode.i_links_count = 0;
                // Reset extent tree to empty leaf.
                let header_ptr = inode.i_block.as_mut_ptr() as *mut Ext4ExtentHeader;
                let header = unsafe { &mut *header_ptr };
                header.eh_magic = Ext4ExtentHeader::MAGIC;
                header.eh_entries = 0;
                header.eh_max = ((15 * 4 - 12) / 12) as u16;
                header.eh_depth = 0;
                header.eh_generation = 0;
            });

            let mut fs = self.fs.lock();
            inode_caches_invalidate(inode_num, &self.block_device);
            fs.dealloc_inode(inode_num);
        } else {
            let links_left = child.modify_disk_inode(|inode| {
                if inode.i_links_count > 0 {
                    inode.i_links_count -= 1;
                }
                inode.i_links_count
            });

            if links_left == 0 {
                // No hard links remain: reclaim inode/data now.
                // Open-unlinked lifetime is handled at the OS layer by deferring
                // the final unlink until the last open fd is dropped.
                child.free_all_blocks()?;
                child.modify_disk_inode(|inode| {
                    inode.i_size_lo = 0;
                    inode.i_size_high = 0;
                    inode.i_blocks_lo = 0;
                    inode.i_links_count = 0;
                    let header_ptr = inode.i_block.as_mut_ptr() as *mut Ext4ExtentHeader;
                    let header = unsafe { &mut *header_ptr };
                    header.eh_magic = Ext4ExtentHeader::MAGIC;
                    header.eh_entries = 0;
                    header.eh_max = ((15 * 4 - 12) / 12) as u16;
                    header.eh_depth = 0;
                    header.eh_generation = 0;
                });

                let mut fs = self.fs.lock();
                inode_caches_invalidate(inode_num, &self.block_device);
                fs.dealloc_inode(inode_num);
            }
        }

        if let Some(idx) = dir_index_cached(self.inode_num, &self.block_device) {
            idx.lock().remove(name);
        }

        Ok(())
    }

    /// Rename an entry within this directory.
    pub fn rename(&self, old: &str, new: &str) -> Result<()> {
        if !self.is_dir() {
            return Err(Ext4Error::NotADirectory);
        }
        if old.is_empty() || new.is_empty() {
            return Err(Ext4Error::InvalidInput);
        }
        if old == "." || old == ".." || new == "." || new == ".." {
            return Err(Ext4Error::InvalidInput);
        }
        if old == new {
            return Ok(());
        }
        if self.find(new).is_some() {
            return Err(Ext4Error::AlreadyExists);
        }

        let child = self.find(old).ok_or(Ext4Error::NotFound)?;
        let inode_num = child.inode_num();
        let file_type = if child.is_dir() {
            Ext4DirEntry::FT_DIR
        } else if child.is_file() {
            Ext4DirEntry::FT_REG_FILE
        } else {
            Ext4DirEntry::FT_UNKNOWN
        };

        self.dir_add_entry(new, inode_num, file_type)?;
        let _ = self.dir_remove_entry(old)?;

        if let Some(idx) = dir_index_cached(self.inode_num, &self.block_device) {
            let mut map = idx.lock();
            map.remove(old);
            map.insert(String::from(new), (inode_num, file_type));
        }
        Ok(())
    }
}

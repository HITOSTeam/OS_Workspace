//! Ext4 filesystem implementation (read-only)

use super::error::{Ext4Error, Result};
use super::layout::*;
use super::vfs::Inode;
use super::{BLOCK_SZ, BlockDevice, get_block_cache};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};

/// Ext4 filesystem structure
pub struct Ext4FileSystem {
    /// Block device
    pub block_device: Arc<dyn BlockDevice>,
    /// Superblock (cached)
    pub superblock: Ext4SuperBlock,
    /// Group descriptors (cached)
    pub group_descs: Vec<Ext4GroupDesc>,
    /// Whether 64-bit mode is enabled
    pub is_64bit: bool,
    /// Mount-time inode-table geometry shared with lockless readers.
    inode_layout: Arc<InodeTableLayout>,
    /// Next block to try allocating (simple sequential cursor).
    next_alloc_block: u64,
    /// Next inode to try allocating (simple sequential cursor).
    next_alloc_inode: u32,
}

/// Immutable inode-table geometry captured when the filesystem is mounted.
///
/// Inode-table locations and sizing fields cannot change during the lifetime
/// of a mounted ext4 filesystem.  Keeping this read-only view outside the
/// allocator lock lets ordinary pathname lookup resolve an inode number
/// without serializing on unrelated block/inode allocation.
struct InodeTableLayout {
    block_size: usize,
    inode_size: usize,
    inodes_per_group: u32,
    inode_table_blocks: Vec<u64>,
}

impl InodeTableLayout {
    fn new(superblock: &Ext4SuperBlock, group_descs: &[Ext4GroupDesc], is_64bit: bool) -> Self {
        Self {
            block_size: superblock.block_size(),
            inode_size: superblock.inode_size(),
            inodes_per_group: superblock.s_inodes_per_group,
            inode_table_blocks: group_descs
                .iter()
                .map(|desc| desc.inode_table(is_64bit))
                .collect(),
        }
    }

    fn inode_pos(&self, inode_num: u32) -> (usize, usize) {
        // Inode numbers start at 1.
        let inode_idx = inode_num - 1;
        let group = inode_idx / self.inodes_per_group;
        let local_idx = inode_idx % self.inodes_per_group;
        let inodes_per_block = self.block_size / self.inode_size;
        let block_offset = local_idx as usize / inodes_per_block;
        let offset_in_block = (local_idx as usize % inodes_per_block) * self.inode_size;

        (
            self.inode_table_blocks[group as usize] as usize + block_offset,
            offset_in_block,
        )
    }
}

/// Cooperative lock for filesystem-wide allocator metadata.
///
/// Linux never sleeps while holding a raw spinlock.  This compact ext4 backend
/// still performs an occasional cold bitmap read while its allocator state is
/// locked, so contenders must yield through `BlockDevice::io_relax()` instead
/// of burning a CPU on `spin::Mutex::lock()`.  The scope is intentionally much
/// narrower than the removed kernel-wide ext4 lock: inode data and directory
/// lookups do not take this lock unless they need shared allocator metadata.
pub struct Ext4FileSystemHandle {
    inner: Mutex<Ext4FileSystem>,
    wait_device: Arc<dyn BlockDevice>,
    inode_layout: Arc<InodeTableLayout>,
}

impl Ext4FileSystemHandle {
    pub fn lock(&self) -> MutexGuard<'_, Ext4FileSystem> {
        loop {
            if let Some(guard) = self.inner.try_lock() {
                return guard;
            }
            self.wait_device.io_relax();
        }
    }

    pub(crate) fn inode_pos(&self, inode_num: u32) -> (usize, usize) {
        self.inode_layout.inode_pos(inode_num)
    }

    pub(crate) fn block_size(&self) -> usize {
        self.inode_layout.block_size
    }
}

impl Ext4FileSystem {
    /// Open an existing ext4 filesystem from block device
    pub fn open(block_device: Arc<dyn BlockDevice>) -> Arc<Ext4FileSystemHandle> {
        match Self::try_open(block_device) {
            Ok(fs) => fs,
            Err(_) => panic!("Invalid ext4 magic number!"),
        }
    }

    /// Attempt to open an ext4 filesystem; returns an error if the superblock is invalid.
    pub fn try_open(block_device: Arc<dyn BlockDevice>) -> Result<Arc<Ext4FileSystemHandle>> {
        // Read superblock (located at byte offset 1024)
        // For 4K blocks, superblock is in block 0 at offset 1024
        // For 1K blocks, superblock is in block 1
        let mut sb_buf = [0u8; SUPERBLOCK_SIZE];

        // Read first 4K block which contains the superblock
        let mut block_buf = [0u8; BLOCK_SZ];
        block_device.read_block(0, &mut block_buf);

        // Copy superblock from offset 1024
        sb_buf.copy_from_slice(&block_buf[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE]);

        let superblock: Ext4SuperBlock =
            unsafe { core::ptr::read(sb_buf.as_ptr() as *const Ext4SuperBlock) };

        if !superblock.is_valid() {
            return Err(Ext4Error::InvalidInput);
        }

        let is_64bit = superblock.s_feature_incompat & EXT4_FEATURE_INCOMPAT_64BIT != 0;
        let desc_size = superblock.desc_size();
        let block_size = superblock.block_size();
        if block_size != BLOCK_SZ {
            return Err(Ext4Error::Unsupported);
        }
        let num_groups = superblock.block_group_count();

        // Read group descriptors
        // GDT is in the block(s) immediately after the superblock
        // For block_size >= 2048, superblock is in block 0, GDT starts at block 1
        // For block_size = 1024, superblock is in block 1, GDT starts at block 2
        let gdt_start_block = if block_size == 1024 { 2 } else { 1 };

        let mut group_descs = Vec::with_capacity(num_groups as usize);

        // Calculate how many descriptors fit in one block
        let descs_per_block = block_size / desc_size;

        for i in 0..num_groups {
            let block_idx = gdt_start_block + (i as usize / descs_per_block);
            let offset_in_block = (i as usize % descs_per_block) * desc_size;

            let cache = get_block_cache(block_idx, Arc::clone(&block_device));
            let gd: Ext4GroupDesc = cache.lock().read(offset_in_block, |gd: &Ext4GroupDesc| *gd);
            group_descs.push(gd);
        }

        let wait_device = Arc::clone(&block_device);
        let inode_layout = Arc::new(InodeTableLayout::new(&superblock, &group_descs, is_64bit));
        Ok(Arc::new(Ext4FileSystemHandle {
            inner: Mutex::new(Self {
                block_device,
                superblock,
                group_descs,
                is_64bit,
                inode_layout: Arc::clone(&inode_layout),
                next_alloc_block: superblock.s_first_data_block as u64,
                next_alloc_inode: superblock.s_first_ino,
            }),
            wait_device,
            inode_layout,
        }))
    }

    /// Get the root inode (inode 2)
    pub fn root_inode(efs: &Arc<Ext4FileSystemHandle>) -> Inode {
        let fs = efs.lock();
        let block_device = Arc::clone(&fs.block_device);
        let (block_id, offset) = fs.get_inode_pos(EXT4_ROOT_INO);
        drop(fs);

        Inode::new(
            EXT4_ROOT_INO,
            block_id,
            offset,
            Arc::clone(efs),
            block_device,
        )
    }

    /// Get the position of an inode (block id and offset within block)
    pub fn get_inode_pos(&self, inode_num: u32) -> (usize, usize) {
        self.inode_layout.inode_pos(inode_num)
    }

    /// Read an inode from disk
    pub fn read_inode(&self, inode_num: u32) -> Ext4Inode {
        let (block_id, offset) = self.get_inode_pos(inode_num);
        let cache = get_block_cache(block_id, Arc::clone(&self.block_device));
        let guard = cache.lock();
        guard.read(offset, |inode: &Ext4Inode| *inode)
    }

    /// Write an inode to disk (updates only the inode table entry).
    pub fn write_inode(&self, inode_num: u32, inode: &Ext4Inode) {
        let (block_id, offset) = self.get_inode_pos(inode_num);
        let cache = get_block_cache(block_id, Arc::clone(&self.block_device));
        let mut guard = cache.lock();
        guard.modify(offset, |disk_inode: &mut Ext4Inode| {
            *disk_inode = *inode;
        });
    }

    /// Get block size
    pub fn block_size(&self) -> usize {
        self.inode_layout.block_size
    }

    fn set_bitmap_bit(bytes: &mut [u8], bit: usize, value: bool) {
        let byte_pos = bit / 8;
        let bit_in_byte = bit % 8;
        let mask = 1u8 << bit_in_byte;
        if value {
            bytes[byte_pos] |= mask;
        } else {
            bytes[byte_pos] &= !mask;
        }
    }

    fn find_and_alloc_bit_from(
        bytes: &mut [u8],
        start_bit: usize,
        total_bits: usize,
    ) -> Option<usize> {
        if total_bits == 0 {
            return None;
        }
        let start = core::cmp::min(start_bit, total_bits);
        for bit in start..total_bits {
            let byte_pos = bit / 8;
            let bit_in_byte = bit % 8;
            if (bytes[byte_pos] & (1u8 << bit_in_byte)) == 0 {
                Self::set_bitmap_bit(bytes, bit, true);
                return Some(bit);
            }
        }
        for bit in 0..start {
            let byte_pos = bit / 8;
            let bit_in_byte = bit % 8;
            if (bytes[byte_pos] & (1u8 << bit_in_byte)) == 0 {
                Self::set_bitmap_bit(bytes, bit, true);
                return Some(bit);
            }
        }
        None
    }

    /// Allocate a free inode and mark it in inode bitmap.
    pub fn alloc_inode(&mut self) -> Option<u32> {
        let inodes_per_group = self.superblock.s_inodes_per_group as usize;
        let start_inode = self.next_alloc_inode.max(1);
        let start_idx = (start_inode - 1) as usize;
        let start_group = start_idx / inodes_per_group;
        let start_local = start_idx % inodes_per_group;

        for pass in 0..2 {
            let groups = self.group_descs.len();
            let begin = if pass == 0 { start_group } else { 0 };
            let end = if pass == 0 {
                groups
            } else {
                core::cmp::min(start_group, groups)
            };
            for group in begin..end {
                let gd = &self.group_descs[group];
                let bitmap_block = gd.inode_bitmap(self.is_64bit) as usize;
                let cache = get_block_cache(bitmap_block, Arc::clone(&self.block_device));
                let mut guard = cache.lock();

                let bit = {
                    let bytes = guard.get_bytes_mut(0, BLOCK_SZ);
                    let start = if pass == 0 && group == start_group {
                        start_local
                    } else {
                        0
                    };
                    Self::find_and_alloc_bit_from(bytes, start, inodes_per_group)
                };

                if let Some(local_bit) = bit {
                    let inode_num =
                        group as u32 * self.superblock.s_inodes_per_group + local_bit as u32 + 1;
                    self.next_alloc_inode = inode_num.saturating_add(1);
                    return Some(inode_num);
                }
            }
        }
        None
    }

    /// Deallocate an inode in inode bitmap.
    pub fn dealloc_inode(&mut self, inode_num: u32) {
        if inode_num == 0 {
            return;
        }
        let inode_idx = inode_num - 1;
        let group = (inode_idx / self.superblock.s_inodes_per_group) as usize;
        let local_idx = (inode_idx % self.superblock.s_inodes_per_group) as usize;
        if group >= self.group_descs.len() {
            return;
        }

        let bitmap_block = self.group_descs[group].inode_bitmap(self.is_64bit) as usize;
        let cache = get_block_cache(bitmap_block, Arc::clone(&self.block_device));
        let mut guard = cache.lock();
        {
            let bytes = guard.get_bytes_mut(0, BLOCK_SZ);
            Self::set_bitmap_bit(bytes, local_idx, false);
        }
    }

    /// Allocate a free block and mark it in block bitmap.
    pub fn alloc_block(&mut self) -> Option<u64> {
        let blocks_per_group = self.superblock.s_blocks_per_group as u64;
        let total_blocks = self.superblock.blocks_count();
        let first_data_block = self.superblock.s_first_data_block as u64;

        let start = self.next_alloc_block.max(first_data_block);
        let start_rel = start - first_data_block;
        let start_group = (start_rel / blocks_per_group) as usize;
        let start_local = (start_rel % blocks_per_group) as usize;

        for pass in 0..2 {
            let groups = self.group_descs.len();
            let begin = if pass == 0 { start_group } else { 0 };
            let end = if pass == 0 {
                groups
            } else {
                core::cmp::min(start_group, groups)
            };
            for group in begin..end {
                let group_start = first_data_block + group as u64 * blocks_per_group;
                if group_start >= total_blocks {
                    break;
                }

                let remaining = (total_blocks - group_start) as usize;
                let bits_in_group = core::cmp::min(remaining, blocks_per_group as usize);

                let gd = &self.group_descs[group];
                let bitmap_block = gd.block_bitmap(self.is_64bit) as usize;
                let cache = get_block_cache(bitmap_block, Arc::clone(&self.block_device));
                let mut guard = cache.lock();

                let bit = {
                    let bytes = guard.get_bytes_mut(0, BLOCK_SZ);
                    let start = if pass == 0 && group == start_group {
                        start_local
                    } else {
                        0
                    };
                    Self::find_and_alloc_bit_from(bytes, start, bits_in_group)
                };

                if let Some(local_bit) = bit {
                    let blk = group_start + local_bit as u64;
                    self.next_alloc_block = blk.saturating_add(1);
                    return Some(blk);
                }
            }
        }
        None
    }

    /// Deallocate a block in block bitmap.
    pub fn dealloc_block(&mut self, block: u64) {
        let blocks_per_group = self.superblock.s_blocks_per_group as u64;
        let first_data_block = self.superblock.s_first_data_block as u64;
        if block < first_data_block {
            return;
        }

        let rel = block - first_data_block;
        let group = (rel / blocks_per_group) as usize;
        let local = (rel % blocks_per_group) as usize;
        if group >= self.group_descs.len() {
            return;
        }

        let bitmap_block = self.group_descs[group].block_bitmap(self.is_64bit) as usize;
        let cache = get_block_cache(bitmap_block, Arc::clone(&self.block_device));
        let mut guard = cache.lock();
        {
            let bytes = guard.get_bytes_mut(0, BLOCK_SZ);
            Self::set_bitmap_bit(bytes, local, false);
        }
    }
}

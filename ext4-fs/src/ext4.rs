//! Ext4 filesystem implementation (read-only)

use super::error::{Ext4Error, Result};
use super::layout::*;
use super::vfs::Inode;
use super::{BLOCK_SZ, BlockDevice, get_block_cache};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

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
    /// Next block to try allocating (simple sequential cursor).
    next_alloc_block: u64,
    /// Next inode to try allocating (simple sequential cursor).
    next_alloc_inode: u32,
}

impl Ext4FileSystem {
    /// Open an existing ext4 filesystem from block device
    pub fn open(block_device: Arc<dyn BlockDevice>) -> Arc<Mutex<Self>> {
        match Self::try_open(block_device) {
            Ok(fs) => fs,
            Err(_) => panic!("Invalid ext4 magic number!"),
        }
    }

    /// Attempt to open an ext4 filesystem; returns an error if the superblock is invalid.
    pub fn try_open(block_device: Arc<dyn BlockDevice>) -> Result<Arc<Mutex<Self>>> {
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

        Ok(Arc::new(Mutex::new(Self {
            block_device,
            superblock,
            group_descs,
            is_64bit,
            next_alloc_block: superblock.s_first_data_block as u64,
            next_alloc_inode: superblock.s_first_ino,
        })))
    }

    /// Get the root inode (inode 2)
    pub fn root_inode(efs: &Arc<Mutex<Self>>) -> Inode {
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
        // Inode numbers start at 1
        let inode_idx = inode_num - 1;
        let group = inode_idx / self.superblock.s_inodes_per_group;
        let local_idx = inode_idx % self.superblock.s_inodes_per_group;

        let inode_size = self.superblock.inode_size();
        let inodes_per_block = self.superblock.block_size() / inode_size;

        let inode_table_block = self.group_descs[group as usize].inode_table(self.is_64bit);
        let block_offset = local_idx as usize / inodes_per_block;
        let offset_in_block = (local_idx as usize % inodes_per_block) * inode_size;

        ((inode_table_block as usize + block_offset), offset_in_block)
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
        self.superblock.block_size()
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

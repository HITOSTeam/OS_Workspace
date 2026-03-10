//! Bitmap operations for ext4 (read-only)

use super::{BlockDevice, BLOCK_SZ};
use alloc::sync::Arc;

/// Number of bits in a block
const BLOCK_BITS: usize = BLOCK_SZ * 8;

/// Bitmap reader for inode/block allocation checks
pub struct Bitmap {
    start_block: u64,
    blocks: usize,
}

impl Bitmap {
    /// Create a new bitmap
    pub fn new(start_block: u64, blocks: usize) -> Self {
        Self {
            start_block,
            blocks,
        }
    }

    /// Check if a bit is set (allocated)
    pub fn is_allocated(&self, bit: usize, block_device: &Arc<dyn BlockDevice>) -> bool {
        let block_pos = bit / BLOCK_BITS;
        let bit_in_block = bit % BLOCK_BITS;
        let byte_pos = bit_in_block / 8;
        let bit_in_byte = bit_in_block % 8;

        if block_pos >= self.blocks {
            return false;
        }

        let mut buf = [0u8; BLOCK_SZ];
        block_device.read_block((self.start_block + block_pos as u64) as usize, &mut buf);

        (buf[byte_pos] & (1 << bit_in_byte)) != 0
    }

    /// Count free bits in the bitmap
    pub fn count_free(&self, total_bits: usize, block_device: &Arc<dyn BlockDevice>) -> usize {
        let mut free = 0;
        let mut buf = [0u8; BLOCK_SZ];

        for block_idx in 0..self.blocks {
            block_device.read_block((self.start_block + block_idx as u64) as usize, &mut buf);

            let bits_in_block = if block_idx == self.blocks - 1 {
                total_bits % BLOCK_BITS
            } else {
                BLOCK_BITS
            };

            for byte_idx in 0..bits_in_block / 8 {
                free += 8 - buf[byte_idx].count_ones() as usize;
            }

            // Handle remaining bits
            let remaining = bits_in_block % 8;
            if remaining > 0 {
                let byte = buf[bits_in_block / 8];
                for i in 0..remaining {
                    if byte & (1 << i) == 0 {
                        free += 1;
                    }
                }
            }
        }

        free
    }
}

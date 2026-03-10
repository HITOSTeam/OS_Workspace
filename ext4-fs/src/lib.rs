//! Ext4 filesystem implementation for embedded/OS kernels
//!
//! This is a simplified ext4 implementation that supports:
//! - Reading ext4 filesystem images
//! - Basic file and directory operations
//! - Extent-based block allocation (ext4 feature)
//!
//! Layout (simplified):
//! ```text
//! +-------------+-------------------+------------------+------------------+
//! | Boot Block  | Block Group 0     | Block Group 1    | ...              |
//! | (1024 bytes)| (superblock, GDT, | (copy of sb/GDT, |                  |
//! |             |  bitmaps, inodes, |  bitmaps, inodes,|                  |
//! |             |  data blocks)     |  data blocks)    |                  |
//! +-------------+-------------------+------------------+------------------+
//! ```

#![no_std]
extern crate alloc;

mod bitmap;
mod block_cache;
mod block_dev;
mod error;
mod ext4;
mod layout;
mod vfs;

pub mod debug;

/// Block size: 4096 bytes (standard for ext4)
pub const BLOCK_SZ: usize = 4096;

pub use block_dev::BlockDevice;
pub use error::{Ext4Error, Result};
pub use ext4::Ext4FileSystem;
pub use vfs::Inode;

pub use block_cache::cache_stats;
use block_cache::{block_cache_sync_all, get_block_cache};

/// Flush all cached blocks to the underlying block device.
pub fn sync_all() {
    block_cache_sync_all();
}

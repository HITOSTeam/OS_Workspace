//! Ext4 on-disk data structures
//!
//! Reference: https://ext4.wiki.kernel.org/index.php/Ext4_Disk_Layout

use super::{BLOCK_SZ, BlockDevice};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::{Debug, Formatter, Result};

/// Ext4 magic number
pub const EXT4_MAGIC: u16 = 0xEF53;

/// Superblock offset from start of filesystem (byte 1024)
pub const SUPERBLOCK_OFFSET: usize = 1024;

/// Size of superblock in bytes
pub const SUPERBLOCK_SIZE: usize = 1024;

/// Root inode number (always 2 in ext4)
pub const EXT4_ROOT_INO: u32 = 2;

/// Maximum file name length
pub const EXT4_NAME_LEN: usize = 255;

/// Inode size (typically 256 bytes in ext4)
pub const EXT4_INODE_SIZE: usize = 256;

// Inode mode flags
pub const S_IFMT: u16 = 0o170000; // File type mask
pub const S_IFSOCK: u16 = 0o140000; // Socket
pub const S_IFLNK: u16 = 0o120000; // Symbolic link
pub const S_IFREG: u16 = 0o100000; // Regular file
pub const S_IFDIR: u16 = 0o040000; // Directory
pub const S_IFBLK: u16 = 0o060000; // Block device
pub const S_IFCHR: u16 = 0o020000; // Character device
pub const S_IFIFO: u16 = 0o010000; // FIFO

// Feature flags
pub const EXT4_FEATURE_INCOMPAT_EXTENTS: u32 = 0x0040;
pub const EXT4_FEATURE_INCOMPAT_64BIT: u32 = 0x0080;
pub const EXT4_FEATURE_INCOMPAT_FLEX_BG: u32 = 0x0200;

// Inode flags
pub const EXT4_EXTENTS_FL: u32 = 0x00080000;

/// Ext4 Superblock (partial, key fields only)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Ext4SuperBlock {
    pub s_inodes_count: u32,         // 0x00: Total inode count
    pub s_blocks_count_lo: u32,      // 0x04: Total block count (low 32 bits)
    pub s_r_blocks_count_lo: u32,    // 0x08: Reserved block count (low)
    pub s_free_blocks_count_lo: u32, // 0x0C: Free block count (low)
    pub s_free_inodes_count: u32,    // 0x10: Free inode count
    pub s_first_data_block: u32,     // 0x14: First data block
    pub s_log_block_size: u32,       // 0x18: Block size = 1024 << s_log_block_size
    pub s_log_cluster_size: u32,     // 0x1C: Cluster size
    pub s_blocks_per_group: u32,     // 0x20: Blocks per group
    pub s_clusters_per_group: u32,   // 0x24: Clusters per group
    pub s_inodes_per_group: u32,     // 0x28: Inodes per group
    pub s_mtime: u32,                // 0x2C: Mount time
    pub s_wtime: u32,                // 0x30: Write time
    pub s_mnt_count: u16,            // 0x34: Mount count
    pub s_max_mnt_count: u16,        // 0x36: Max mount count
    pub s_magic: u16,                // 0x38: Magic signature (0xEF53)
    pub s_state: u16,                // 0x3A: File system state
    pub s_errors: u16,               // 0x3C: Error behavior
    pub s_minor_rev_level: u16,      // 0x3E: Minor revision level
    pub s_lastcheck: u32,            // 0x40: Last check time
    pub s_checkinterval: u32,        // 0x44: Check interval
    pub s_creator_os: u32,           // 0x48: OS type
    pub s_rev_level: u32,            // 0x4C: Revision level
    pub s_def_resuid: u16,           // 0x50: Default UID for reserved blocks
    pub s_def_resgid: u16,           // 0x52: Default GID for reserved blocks
    // EXT4_DYNAMIC_REV specific
    pub s_first_ino: u32,              // 0x54: First non-reserved inode
    pub s_inode_size: u16,             // 0x58: Size of inode structure
    pub s_block_group_nr: u16,         // 0x5A: Block group number of this superblock
    pub s_feature_compat: u32,         // 0x5C: Compatible feature set
    pub s_feature_incompat: u32,       // 0x60: Incompatible feature set
    pub s_feature_ro_compat: u32,      // 0x64: Readonly-compatible feature set
    pub s_uuid: [u8; 16],              // 0x68: 128-bit UUID
    pub s_volume_name: [u8; 16],       // 0x78: Volume name
    pub s_last_mounted: [u8; 64],      // 0x88: Directory where last mounted
    pub s_algorithm_usage_bitmap: u32, // 0xC8: For compression
    // Performance hints
    pub s_prealloc_blocks: u8,      // 0xCC: Blocks to preallocate for files
    pub s_prealloc_dir_blocks: u8,  // 0xCD: Blocks to preallocate for dirs
    pub s_reserved_gdt_blocks: u16, // 0xCE: Reserved GDT blocks for growth
    // Journaling
    pub s_journal_uuid: [u8; 16],  // 0xD0: UUID of journal superblock
    pub s_journal_inum: u32,       // 0xE0: Inode number of journal file
    pub s_journal_dev: u32,        // 0xE4: Device number of journal file
    pub s_last_orphan: u32,        // 0xE8: Start of list of orphan inodes
    pub s_hash_seed: [u32; 4],     // 0xEC: HTREE hash seed
    pub s_def_hash_version: u8,    // 0xFC: Default hash version
    pub s_jnl_backup_type: u8,     // 0xFD: Journal backup type
    pub s_desc_size: u16,          // 0xFE: Size of group descriptor
    pub s_default_mount_opts: u32, // 0x100: Default mount options
    pub s_first_meta_bg: u32,      // 0x104: First metablock block group
    pub s_mkfs_time: u32,          // 0x108: Filesystem creation time
    pub s_jnl_blocks: [u32; 17],   // 0x10C: Backup of journal inodes
    // 64-bit support
    pub s_blocks_count_hi: u32,      // 0x150: High 32-bits of block count
    pub s_r_blocks_count_hi: u32,    // 0x154: High 32-bits of reserved blocks
    pub s_free_blocks_count_hi: u32, // 0x158: High 32-bits of free blocks
    pub s_min_extra_isize: u16,      // 0x15C: All inodes have at least this
    pub s_want_extra_isize: u16,     // 0x15E: New inodes should reserve this
    pub s_flags: u32,                // 0x160: Miscellaneous flags
    pub s_raid_stride: u16,          // 0x164: RAID stride
    pub s_mmp_interval: u16,         // 0x166: MMP check interval
    pub s_mmp_block: u64,            // 0x168: Block for MMP
    pub s_raid_stripe_width: u32,    // 0x170: Blocks on all data disks
    pub s_log_groups_per_flex: u8,   // 0x174: FLEX_BG group size
    pub s_checksum_type: u8,         // 0x175: Metadata checksum algorithm
    pub s_reserved_pad: u16,         // 0x176: Padding
    pub s_kbytes_written: u64,       // 0x178: KB written to fs
    // More fields exist but we only need the basics
    _padding: [u8; 104], // Padding to reach 0x200 (512 bytes)
}

impl Ext4SuperBlock {
    /// Check if the superblock is valid
    pub fn is_valid(&self) -> bool {
        self.s_magic == EXT4_MAGIC
    }

    /// Get block size in bytes
    pub fn block_size(&self) -> usize {
        1024 << self.s_log_block_size
    }

    /// Get total block count (64-bit)
    pub fn blocks_count(&self) -> u64 {
        (self.s_blocks_count_hi as u64) << 32 | self.s_blocks_count_lo as u64
    }

    /// Get inode size
    pub fn inode_size(&self) -> usize {
        if self.s_rev_level > 0 {
            self.s_inode_size as usize
        } else {
            128 // Old revision
        }
    }

    /// Get group descriptor size
    pub fn desc_size(&self) -> usize {
        if self.s_feature_incompat & EXT4_FEATURE_INCOMPAT_64BIT != 0 && self.s_desc_size > 32 {
            self.s_desc_size as usize
        } else {
            32
        }
    }

    /// Check if extent feature is enabled
    pub fn has_extents(&self) -> bool {
        self.s_feature_incompat & EXT4_FEATURE_INCOMPAT_EXTENTS != 0
    }

    /// Get number of block groups
    pub fn block_group_count(&self) -> u32 {
        let blocks = self.blocks_count();
        ((blocks + self.s_blocks_per_group as u64 - 1) / self.s_blocks_per_group as u64) as u32
    }
}

impl Debug for Ext4SuperBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.debug_struct("Ext4SuperBlock")
            .field("magic", &format_args!("0x{:04X}", self.s_magic))
            .field("block_size", &self.block_size())
            .field("blocks_count", &self.blocks_count())
            .field("inodes_count", &self.s_inodes_count)
            .field("blocks_per_group", &self.s_blocks_per_group)
            .field("inodes_per_group", &self.s_inodes_per_group)
            .field("inode_size", &self.inode_size())
            .field("has_extents", &self.has_extents())
            .finish()
    }
}

/// Block Group Descriptor (32-byte or 64-byte version)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Ext4GroupDesc {
    pub bg_block_bitmap_lo: u32,      // 0x00: Block bitmap block (low)
    pub bg_inode_bitmap_lo: u32,      // 0x04: Inode bitmap block (low)
    pub bg_inode_table_lo: u32,       // 0x08: Inode table block (low)
    pub bg_free_blocks_count_lo: u16, // 0x0C: Free block count (low)
    pub bg_free_inodes_count_lo: u16, // 0x0E: Free inode count (low)
    pub bg_used_dirs_count_lo: u16,   // 0x10: Directory count (low)
    pub bg_flags: u16,                // 0x12: Flags
    pub bg_exclude_bitmap_lo: u32,    // 0x14: Exclude bitmap (low)
    pub bg_block_bitmap_csum_lo: u16, // 0x18: Block bitmap checksum (low)
    pub bg_inode_bitmap_csum_lo: u16, // 0x1A: Inode bitmap checksum (low)
    pub bg_itable_unused_lo: u16,     // 0x1C: Unused inode count (low)
    pub bg_checksum: u16,             // 0x1E: Group descriptor checksum
    // 64-bit fields (if desc_size > 32)
    pub bg_block_bitmap_hi: u32,      // 0x20: Block bitmap block (high)
    pub bg_inode_bitmap_hi: u32,      // 0x24: Inode bitmap block (high)
    pub bg_inode_table_hi: u32,       // 0x28: Inode table block (high)
    pub bg_free_blocks_count_hi: u16, // 0x2C: Free block count (high)
    pub bg_free_inodes_count_hi: u16, // 0x2E: Free inode count (high)
    pub bg_used_dirs_count_hi: u16,   // 0x30: Directory count (high)
    pub bg_itable_unused_hi: u16,     // 0x32: Unused inode count (high)
    pub bg_exclude_bitmap_hi: u32,    // 0x34: Exclude bitmap (high)
    pub bg_block_bitmap_csum_hi: u16, // 0x38: Block bitmap checksum (high)
    pub bg_inode_bitmap_csum_hi: u16, // 0x3A: Inode bitmap checksum (high)
    pub bg_reserved: u32,             // 0x3C: Reserved
}

impl Ext4GroupDesc {
    /// Get inode table block address
    pub fn inode_table(&self, is_64bit: bool) -> u64 {
        if is_64bit {
            (self.bg_inode_table_hi as u64) << 32 | self.bg_inode_table_lo as u64
        } else {
            self.bg_inode_table_lo as u64
        }
    }

    /// Get block bitmap block address
    pub fn block_bitmap(&self, is_64bit: bool) -> u64 {
        if is_64bit {
            (self.bg_block_bitmap_hi as u64) << 32 | self.bg_block_bitmap_lo as u64
        } else {
            self.bg_block_bitmap_lo as u64
        }
    }

    /// Get inode bitmap block address
    pub fn inode_bitmap(&self, is_64bit: bool) -> u64 {
        if is_64bit {
            (self.bg_inode_bitmap_hi as u64) << 32 | self.bg_inode_bitmap_lo as u64
        } else {
            self.bg_inode_bitmap_lo as u64
        }
    }
}

/// Ext4 Inode structure (128-byte base + extra)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Ext4Inode {
    pub i_mode: u16,        // 0x00: File mode
    pub i_uid: u16,         // 0x02: Owner UID (low)
    pub i_size_lo: u32,     // 0x04: Size in bytes (low)
    pub i_atime: u32,       // 0x08: Access time
    pub i_ctime: u32,       // 0x0C: Inode change time
    pub i_mtime: u32,       // 0x10: Modification time
    pub i_dtime: u32,       // 0x14: Deletion time
    pub i_gid: u16,         // 0x18: Group ID (low)
    pub i_links_count: u16, // 0x1A: Hard link count
    pub i_blocks_lo: u32,   // 0x1C: Block count (low)
    pub i_flags: u32,       // 0x20: Flags
    pub i_osd1: u32,        // 0x24: OS dependent
    pub i_block: [u32; 15], // 0x28: Block pointers or extent tree
    pub i_generation: u32,  // 0x64: File version
    pub i_file_acl_lo: u32, // 0x68: Extended attribute block (low)
    pub i_size_high: u32,   // 0x6C: Size in bytes (high)
    pub i_obso_faddr: u32,  // 0x70: Obsolete fragment address
    // OS dependent 2 (12 bytes)
    pub i_osd2: [u8; 12],    // 0x74: OS dependent 2
    pub i_extra_isize: u16,  // 0x80: Extra inode size
    pub i_checksum_hi: u16,  // 0x82: Inode checksum (high)
    pub i_ctime_extra: u32,  // 0x84: Extra change time
    pub i_mtime_extra: u32,  // 0x88: Extra modification time
    pub i_atime_extra: u32,  // 0x8C: Extra access time
    pub i_crtime: u32,       // 0x90: Creation time
    pub i_crtime_extra: u32, // 0x94: Extra creation time
    pub i_version_hi: u32,   // 0x98: Version (high)
    pub i_projid: u32,       // 0x9C: Project ID
    _padding: [u8; 96],      // Padding to 256 bytes
}

impl Ext4Inode {
    /// Check if this is a regular file
    pub fn is_file(&self) -> bool {
        (self.i_mode & S_IFMT) == S_IFREG
    }

    /// Check if this is a directory
    pub fn is_dir(&self) -> bool {
        (self.i_mode & S_IFMT) == S_IFDIR
    }

    /// Check if this is a symbolic link
    pub fn is_symlink(&self) -> bool {
        (self.i_mode & S_IFMT) == S_IFLNK
    }

    /// Check if this is a FIFO.
    pub fn is_fifo(&self) -> bool {
        (self.i_mode & S_IFMT) == S_IFIFO
    }

    /// Check if this is a character device.
    pub fn is_chrdev(&self) -> bool {
        (self.i_mode & S_IFMT) == S_IFCHR
    }

    /// Check if this is a block device.
    pub fn is_blkdev(&self) -> bool {
        (self.i_mode & S_IFMT) == S_IFBLK
    }

    /// Check if this is a socket inode.
    pub fn is_socket(&self) -> bool {
        (self.i_mode & S_IFMT) == S_IFSOCK
    }

    /// Get file size (64-bit)
    pub fn size(&self) -> u64 {
        (self.i_size_high as u64) << 32 | self.i_size_lo as u64
    }

    /// Check if inode uses extents
    pub fn uses_extents(&self) -> bool {
        self.i_flags & EXT4_EXTENTS_FL != 0
    }

    /// Get the i_block array as bytes for extent parsing
    pub fn block_data(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self.i_block.as_ptr() as *const u8,
                60, // 15 * 4 bytes
            )
        }
    }

    /// Get the i_block array as mutable bytes (for fast symlinks).
    pub fn block_data_mut(&mut self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.i_block.as_mut_ptr() as *mut u8,
                60, // 15 * 4 bytes
            )
        }
    }
}

impl Debug for Ext4Inode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.debug_struct("Ext4Inode")
            .field("mode", &format_args!("0o{:06o}", self.i_mode))
            .field("size", &self.size())
            .field("is_dir", &self.is_dir())
            .field("is_file", &self.is_file())
            .field("uses_extents", &self.uses_extents())
            .field("links_count", &self.i_links_count)
            .finish()
    }
}

/// Extent header (at start of extent tree node)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Ext4ExtentHeader {
    pub eh_magic: u16,      // Magic number: 0xF30A
    pub eh_entries: u16,    // Number of valid entries
    pub eh_max: u16,        // Capacity of entries
    pub eh_depth: u16,      // Depth of tree (0 = leaf)
    pub eh_generation: u32, // Generation of tree
}

impl Ext4ExtentHeader {
    pub const MAGIC: u16 = 0xF30A;

    pub fn is_valid(&self) -> bool {
        self.eh_magic == Self::MAGIC
    }
}

/// Extent index (internal node of extent tree)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Ext4ExtentIdx {
    pub ei_block: u32,   // First logical block this index covers
    pub ei_leaf_lo: u32, // Block of next level (low)
    pub ei_leaf_hi: u16, // Block of next level (high)
    pub ei_unused: u16,  // Unused
}

impl Ext4ExtentIdx {
    /// Get physical block number of next level
    pub fn leaf_block(&self) -> u64 {
        (self.ei_leaf_hi as u64) << 32 | self.ei_leaf_lo as u64
    }
}

/// Extent (leaf node of extent tree)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Ext4Extent {
    pub ee_block: u32,    // First logical block extent covers
    pub ee_len: u16,      // Number of blocks covered (max 32768)
    pub ee_start_hi: u16, // Physical block number (high)
    pub ee_start_lo: u32, // Physical block number (low)
}

impl Ext4Extent {
    const INIT_MAX_LEN: u16 = 1 << 15;

    /// Get physical start block
    pub fn start_block(&self) -> u64 {
        (self.ee_start_hi as u64) << 32 | self.ee_start_lo as u64
    }

    /// Get extent length (number of blocks)
    pub fn len(&self) -> u32 {
        // ext4 reserves values above 32768 for unwritten extents.  The value
        // 32768 itself is the maximum initialized extent length, so masking
        // the high bit would incorrectly turn a valid 128 MiB extent into an
        // empty one.
        if self.ee_len <= Self::INIT_MAX_LEN {
            self.ee_len as u32
        } else {
            (self.ee_len - Self::INIT_MAX_LEN) as u32
        }
    }

    /// Whether this extent is allocated but has not been initialized.
    pub fn is_unwritten(&self) -> bool {
        self.ee_len > Self::INIT_MAX_LEN
    }
}

#[cfg(test)]
mod extent_tests {
    use super::Ext4Extent;

    fn extent(ee_len: u16) -> Ext4Extent {
        Ext4Extent {
            ee_block: 0,
            ee_len,
            ee_start_hi: 0,
            ee_start_lo: 0,
        }
    }

    #[test]
    fn initialized_extent_can_use_full_32768_blocks() {
        let extent = extent(0x8000);
        assert_eq!(extent.len(), 32768);
        assert!(!extent.is_unwritten());
    }

    #[test]
    fn unwritten_extent_decodes_length_after_flag() {
        let extent = extent(0x8001);
        assert_eq!(extent.len(), 1);
        assert!(extent.is_unwritten());
    }
}

/// Directory entry structure (variable length)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Ext4DirEntry {
    pub inode: u32,   // Inode number
    pub rec_len: u16, // Directory entry length
    pub name_len: u8, // Name length
    pub file_type: u8, // File type
                      // name follows (up to 255 bytes)
}

impl Ext4DirEntry {
    /// File type constants
    pub const FT_UNKNOWN: u8 = 0;
    pub const FT_REG_FILE: u8 = 1;
    pub const FT_DIR: u8 = 2;
    pub const FT_CHRDEV: u8 = 3;
    pub const FT_BLKDEV: u8 = 4;
    pub const FT_FIFO: u8 = 5;
    pub const FT_SOCK: u8 = 6;
    pub const FT_SYMLINK: u8 = 7;

    /// Size of fixed part of directory entry
    pub const FIXED_SIZE: usize = 8;
}

impl Debug for Ext4DirEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.debug_struct("Ext4DirEntry")
            .field("inode", &self.inode)
            .field("rec_len", &self.rec_len)
            .field("name_len", &self.name_len)
            .field("file_type", &self.file_type)
            .finish()
    }
}

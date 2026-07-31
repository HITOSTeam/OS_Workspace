//! Block device trait for ext4 filesystem

use core::any::Any;
use core::hint::spin_loop;

/// Trait for block devices
/// which reads and writes data in the unit of blocks
pub trait BlockDevice: Send + Sync + Any {
    /// Relinquish execution while waiting for block-I/O related progress.
    ///
    /// Standalone users default to a processor hint. Kernels may override this
    /// hook with a scheduler-aware yield so cache-fill waiters do not burn a
    /// CPU while another task owns the I/O.
    fn io_relax(&self) {
        spin_loop();
    }

    /// Read data from block to buffer
    fn read_block(&self, block_id: usize, buf: &mut [u8]);
    /// Write data from buffer to block
    fn write_block(&self, block_id: usize, buf: &[u8]);
    /// Read multiple contiguous blocks starting at `block_id`.
    fn read_blocks(&self, block_id: usize, buf: &mut [u8]) {
        assert!(buf.len() % crate::BLOCK_SZ == 0);
        for (i, chunk) in buf.chunks_mut(crate::BLOCK_SZ).enumerate() {
            self.read_block(block_id + i, chunk);
        }
    }
    /// Write multiple contiguous blocks starting at `block_id`.
    fn write_blocks(&self, block_id: usize, buf: &[u8]) {
        assert!(buf.len() % crate::BLOCK_SZ == 0);
        for (i, chunk) in buf.chunks(crate::BLOCK_SZ).enumerate() {
            self.write_block(block_id + i, chunk);
        }
    }
}

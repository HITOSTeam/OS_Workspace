//! Block cache manager for ext4 filesystem

use super::{BLOCK_SZ, BlockDevice};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;

/// Cached block inside memory
pub struct BlockCache {
    /// cached block data (heap allocated to avoid stack overflow)
    cache: Vec<u8>,
    /// underlying block id
    block_id: usize,
    /// underlying block device
    block_device: Arc<dyn BlockDevice>,
    /// whether the block is dirty
    modified: bool,
}

static CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static CACHE_MISSES: AtomicU64 = AtomicU64::new(0);

pub fn cache_stats() -> (u64, u64) {
    (
        CACHE_HITS.load(Ordering::Relaxed),
        CACHE_MISSES.load(Ordering::Relaxed),
    )
}

impl BlockCache {
    /// Load a new BlockCache from disk
    pub fn new(block_id: usize, block_device: Arc<dyn BlockDevice>) -> Self {
        // Allocate buffer on heap to avoid stack overflow
        let mut cache = vec![0u8; BLOCK_SZ];
        block_device.read_block(block_id, &mut cache);
        Self {
            cache,
            block_id,
            block_device,
            modified: false,
        }
    }

    /// Create a BlockCache from caller-provided block data.
    pub fn new_with_data(block_id: usize, block_device: Arc<dyn BlockDevice>, data: &[u8]) -> Self {
        assert_eq!(data.len(), BLOCK_SZ);
        let mut cache = vec![0u8; BLOCK_SZ];
        cache.copy_from_slice(data);
        Self {
            cache,
            block_id,
            block_device,
            modified: false,
        }
    }

    /// Create a new BlockCache without reading the old on-disk contents.
    ///
    /// This is useful for freshly allocated blocks, where we know the logical
    /// contents should start as zeros and reading the old disk data is wasted.
    pub fn new_zeroed(block_id: usize, block_device: Arc<dyn BlockDevice>) -> Self {
        let cache = vec![0u8; BLOCK_SZ];
        Self {
            cache,
            block_id,
            block_device,
            // Mark dirty because on-disk data may contain stale bytes from a
            // previously freed block.
            modified: true,
        }
    }

    /// Get the address of an offset inside the cached block data
    fn addr_of_offset(&self, offset: usize) -> usize {
        &self.cache[offset] as *const _ as usize
    }

    /// Get immutable reference to data at offset
    pub fn get_ref<T>(&self, offset: usize) -> &T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        let addr = self.addr_of_offset(offset);
        unsafe { &*(addr as *const T) }
    }

    /// Get mutable reference to data at offset
    pub fn get_mut<T>(&mut self, offset: usize) -> &mut T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        self.modified = true;
        let addr = self.addr_of_offset(offset);
        unsafe { &mut *(addr as *mut T) }
    }

    /// Read data at offset with closure
    pub fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        f(self.get_ref(offset))
    }

    /// Modify data at offset with closure
    pub fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        f(self.get_mut(offset))
    }

    /// Get raw byte slice from cache
    pub fn get_bytes(&self, offset: usize, len: usize) -> &[u8] {
        &self.cache[offset..offset + len]
    }

    /// Get raw mutable byte slice from cache
    pub fn get_bytes_mut(&mut self, offset: usize, len: usize) -> &mut [u8] {
        self.modified = true;
        &mut self.cache[offset..offset + len]
    }

    /// Sync cache to disk
    pub fn sync(&mut self) {
        if self.modified {
            self.modified = false;
            self.block_device.write_block(self.block_id, &self.cache);
        }
    }
}

impl Drop for BlockCache {
    fn drop(&mut self) {
        self.sync()
    }
}

/// Block cache size.
///
/// Use a larger cache on LoongArch to reduce cold-start IO for glibc assets.
const BLOCK_CACHE_SIZE: usize = if cfg!(target_arch = "loongarch64") {
    2048
} else {
    512
};

/// Read-ahead blocks on cache miss.
const READ_AHEAD_BLOCKS: usize = if cfg!(target_arch = "loongarch64") {
    2
} else {
    1
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CacheKey {
    block_id: usize,
    device_id: usize,
}

fn device_id(block_device: &Arc<dyn BlockDevice>) -> usize {
    Arc::as_ptr(block_device) as *const () as usize
}

fn cache_key(block_id: usize, block_device: &Arc<dyn BlockDevice>) -> CacheKey {
    CacheKey {
        block_id,
        device_id: device_id(block_device),
    }
}

struct CacheEntry {
    cache: Arc<Mutex<BlockCache>>,
    stamp: u64,
}

/// Block cache manager
pub struct BlockCacheManager {
    entries: BTreeMap<CacheKey, CacheEntry>,
    queue: VecDeque<(CacheKey, u64)>,
    stamp: u64,
}

impl BlockCacheManager {
    /// Create new block cache manager
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            queue: VecDeque::new(),
            stamp: 0,
        }
    }

    fn compact_queue(&mut self) {
        const QUEUE_LIMIT_MULT: usize = 8;
        let limit = BLOCK_CACHE_SIZE.saturating_mul(QUEUE_LIMIT_MULT);
        if self.queue.len() <= limit {
            return;
        }
        let mut new_queue = VecDeque::with_capacity(self.entries.len());
        for (key, entry) in self.entries.iter() {
            new_queue.push_back((*key, entry.stamp));
        }
        self.queue = new_queue;
    }

    fn next_stamp(&mut self) -> u64 {
        self.stamp = self.stamp.wrapping_add(1);
        self.stamp
    }

    fn mark_used(&mut self, key: CacheKey) {
        let stamp = self.next_stamp();
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.stamp = stamp;
        }
        self.queue.push_back((key, stamp));
        self.compact_queue();
    }

    fn evict_one(&mut self) {
        let mut scanned = 0usize;
        let limit = self.queue.len().saturating_add(1);
        while let Some((key, stamp)) = self.queue.pop_front() {
            scanned += 1;
            if let Some(entry) = self.entries.get(&key) {
                if entry.stamp == stamp && Arc::strong_count(&entry.cache) == 1 {
                    self.entries.remove(&key);
                    return;
                }
            }
            if scanned >= limit {
                break;
            }
        }
        panic!("Run out of BlockCache!");
    }

    fn ensure_capacity(&mut self) {
        while self.entries.len() >= BLOCK_CACHE_SIZE {
            self.evict_one();
        }
    }

    fn load_block_range(
        &mut self,
        block_device: Arc<dyn BlockDevice>,
        start_block: usize,
        count: usize,
    ) -> Arc<Mutex<BlockCache>> {
        let count = count.max(1);
        let total_bytes = count.saturating_mul(BLOCK_SZ);
        let mut buf = vec![0u8; total_bytes];
        block_device.read_blocks(start_block, &mut buf);

        let mut first_cache: Option<Arc<Mutex<BlockCache>>> = None;
        for i in 0..count {
            let block_id = start_block + i;
            let key = cache_key(block_id, &block_device);
            if let Some(entry) = self.entries.get(&key) {
                if i == 0 {
                    first_cache = Some(Arc::clone(&entry.cache));
                }
                self.mark_used(key);
                continue;
            }
            self.ensure_capacity();
            let offset = i * BLOCK_SZ;
            let block_cache = Arc::new(Mutex::new(BlockCache::new_with_data(
                block_id,
                Arc::clone(&block_device),
                &buf[offset..offset + BLOCK_SZ],
            )));
            self.entries.insert(
                key,
                CacheEntry {
                    cache: Arc::clone(&block_cache),
                    stamp: 0,
                },
            );
            self.mark_used(key);
            if i == 0 {
                first_cache = Some(Arc::clone(&block_cache));
            }
        }
        first_cache.expect("missing requested block cache")
    }

    /// Get block cache for given block id
    pub fn get_block_cache(
        &mut self,
        block_id: usize,
        block_device: Arc<dyn BlockDevice>,
    ) -> Arc<Mutex<BlockCache>> {
        let key = cache_key(block_id, &block_device);
        if let Some(entry) = self.entries.get(&key) {
            let cache_clone = Arc::clone(&entry.cache);
            self.mark_used(key);
            CACHE_HITS.fetch_add(1, Ordering::Relaxed);
            return cache_clone;
        }
        CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
        self.load_block_range(block_device, block_id, READ_AHEAD_BLOCKS)
    }

    /// Get block cache for given block id without reading old disk contents when absent.
    pub fn get_block_cache_zeroed(
        &mut self,
        block_id: usize,
        block_device: Arc<dyn BlockDevice>,
    ) -> Arc<Mutex<BlockCache>> {
        let key = cache_key(block_id, &block_device);
        if let Some(entry) = self.entries.get(&key) {
            let cache_clone = Arc::clone(&entry.cache);
            self.mark_used(key);
            CACHE_HITS.fetch_add(1, Ordering::Relaxed);
            return cache_clone;
        }
        CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
        self.ensure_capacity();
        let block_cache = Arc::new(Mutex::new(BlockCache::new_zeroed(block_id, block_device)));
        self.entries.insert(
            key,
            CacheEntry {
                cache: Arc::clone(&block_cache),
                stamp: 0,
            },
        );
        self.mark_used(key);
        block_cache
    }
}

lazy_static! {
    /// Global block cache manager instance
    pub static ref BLOCK_CACHE_MANAGER: Mutex<BlockCacheManager> =
        Mutex::new(BlockCacheManager::new());
}

/// Get block cache for given block id
pub fn get_block_cache(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
) -> Arc<Mutex<BlockCache>> {
    let mut manager = BLOCK_CACHE_MANAGER.lock();
    manager.get_block_cache(block_id, block_device)
}

/// Get a zero-initialized block cache for a freshly allocated block.
pub fn get_block_cache_zeroed(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
) -> Arc<Mutex<BlockCache>> {
    let mut manager = BLOCK_CACHE_MANAGER.lock();
    manager.get_block_cache_zeroed(block_id, block_device)
}

/// Sync all block caches to disk
pub fn block_cache_sync_all() {
    let manager = BLOCK_CACHE_MANAGER.lock();
    for entry in manager.entries.values() {
        entry.cache.lock().sync();
    }
}

//! Block cache manager for ext4 filesystem

use super::{BLOCK_SZ, BlockDevice};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::hash::{BuildHasherDefault, Hash, Hasher};
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize as CoreAtomicUsize, Ordering};
use hashbrown::HashMap;
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
    /// Generation most recently modified in memory.
    dirty_generation: u64,
    /// Generation most recently written to the device.
    synced_generation: u64,
    /// Serialize writeback without holding the cache data lock across I/O.
    writeback_in_progress: bool,
}

struct Writeback {
    generation: u64,
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
    data: Vec<u8>,
}

enum WritebackPreparation {
    Done,
    Busy(Arc<dyn BlockDevice>),
    Ready(Writeback),
}

static CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static CACHE_LOADS: AtomicU64 = AtomicU64::new(0);
static CACHE_COALESCED_WAITS: AtomicU64 = AtomicU64::new(0);
static CACHE_WAIT_RETRIES: AtomicU64 = AtomicU64::new(0);
static CACHE_EVICTIONS: AtomicU64 = AtomicU64::new(0);
static CACHE_CLEAN_EVICTIONS: AtomicU64 = AtomicU64::new(0);
static CACHE_DIRTY_EVICTIONS: AtomicU64 = AtomicU64::new(0);
static CACHE_PREFETCHED_BLOCKS: AtomicU64 = AtomicU64::new(0);
static CACHE_ENTRIES: CoreAtomicUsize = CoreAtomicUsize::new(0);

pub fn cache_stats() -> (u64, u64) {
    (
        CACHE_HITS.load(Ordering::Relaxed),
        CACHE_MISSES.load(Ordering::Relaxed),
    )
}

/// Detailed block-cache counters for performance diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheDiagnostics {
    pub hits: u64,
    pub misses: u64,
    pub loads: u64,
    pub coalesced_waits: u64,
    pub wait_retries: u64,
    pub evictions: u64,
    pub clean_evictions: u64,
    pub dirty_evictions: u64,
    pub prefetched_blocks: u64,
    pub entries: usize,
    pub capacity: usize,
}

pub fn cache_diagnostics() -> CacheDiagnostics {
    CacheDiagnostics {
        hits: CACHE_HITS.load(Ordering::Relaxed),
        misses: CACHE_MISSES.load(Ordering::Relaxed),
        loads: CACHE_LOADS.load(Ordering::Relaxed),
        coalesced_waits: CACHE_COALESCED_WAITS.load(Ordering::Relaxed),
        wait_retries: CACHE_WAIT_RETRIES.load(Ordering::Relaxed),
        evictions: CACHE_EVICTIONS.load(Ordering::Relaxed),
        clean_evictions: CACHE_CLEAN_EVICTIONS.load(Ordering::Relaxed),
        dirty_evictions: CACHE_DIRTY_EVICTIONS.load(Ordering::Relaxed),
        prefetched_blocks: CACHE_PREFETCHED_BLOCKS.load(Ordering::Relaxed),
        // Diagnostics must remain observable when the cache is under pressure.
        // Taking the global manager lock here can starve behind filesystem
        // workers and make /proc/perf itself appear hung.
        entries: CACHE_ENTRIES.load(Ordering::Relaxed),
        capacity: block_cache_capacity(),
    }
}

impl BlockCache {
    /// 仅供缓存单元测试从块设备加载完整数据块。
    #[cfg(test)]
    pub fn new(block_id: usize, block_device: Arc<dyn BlockDevice>) -> Self {
        // Allocate buffer on heap to avoid stack overflow
        let mut cache = vec![0u8; BLOCK_SZ];
        block_device.read_block(block_id, &mut cache);
        Self {
            cache,
            block_id,
            block_device,
            dirty_generation: 0,
            synced_generation: 0,
            writeback_in_progress: false,
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
            dirty_generation: 0,
            synced_generation: 0,
            writeback_in_progress: false,
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
            dirty_generation: 1,
            synced_generation: 0,
            writeback_in_progress: false,
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty_generation = self.dirty_generation.saturating_add(1);
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
        self.mark_dirty();
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
        self.mark_dirty();
        &mut self.cache[offset..offset + len]
    }

    fn writeback_target(&self) -> u64 {
        self.dirty_generation
    }

    fn is_clean(&self) -> bool {
        self.synced_generation >= self.dirty_generation && !self.writeback_in_progress
    }

    fn prepare_writeback(&mut self, target: u64) -> WritebackPreparation {
        if self.synced_generation >= target {
            return WritebackPreparation::Done;
        }
        if self.writeback_in_progress {
            return WritebackPreparation::Busy(Arc::clone(&self.block_device));
        }
        self.writeback_in_progress = true;
        WritebackPreparation::Ready(Writeback {
            generation: self.dirty_generation,
            block_id: self.block_id,
            block_device: Arc::clone(&self.block_device),
            data: self.cache.clone(),
        })
    }

    fn finish_writeback(&mut self, generation: u64) {
        debug_assert!(self.writeback_in_progress);
        self.synced_generation = self.synced_generation.max(generation);
        self.writeback_in_progress = false;
    }
}

/// Boot-time floor retained for small-memory systems.
const DEFAULT_BLOCK_CACHE_CAPACITY: usize = if cfg!(target_arch = "loongarch64") {
    2048
} else {
    512
};

/// The block cache stores a second, heap-backed copy of each 4 KiB block, so
/// keep its transitional budget below both a small fraction of RAM and a
/// conservative fraction of the fixed kernel heap.
// Linux can let the page cache consume most free memory because lookup,
// locking and reclaim are split across XArrays, folios and per-node LRUs.
// This transitional cache duplicates whole blocks in the fixed 512 MiB kernel
// heap, so it cannot consume Linux-sized fractions of RAM until it is unified
// with the frame-backed page cache.  Scale to 1/64 of detected RAM, capped at
// 64 MiB: on the final 8 GiB machine that is 16,384 blocks, eight times the old
// 8 MiB budget while retaining more than 100 MiB of measured heap headroom.
const BLOCK_CACHE_MEMORY_DIVISOR: usize = 64;
const MAX_BLOCK_CACHE_CAPACITY: usize = 16 * 1024;
const CLEAN_RECLAIM_SCAN_LIMIT: usize = 64;

#[cfg(not(test))]
static BLOCK_CACHE_CAPACITY: CoreAtomicUsize = CoreAtomicUsize::new(DEFAULT_BLOCK_CACHE_CAPACITY);

fn capacity_for_memory(memory_bytes: usize) -> usize {
    let proportional = memory_bytes / BLOCK_CACHE_MEMORY_DIVISOR / BLOCK_SZ;
    proportional.clamp(DEFAULT_BLOCK_CACHE_CAPACITY, MAX_BLOCK_CACHE_CAPACITY)
}

#[cfg(not(test))]
fn block_cache_capacity() -> usize {
    BLOCK_CACHE_CAPACITY.load(Ordering::Relaxed)
}

#[cfg(test)]
fn block_cache_capacity() -> usize {
    8
}

/// Set the boot-time cache budget from detected physical memory.
///
/// Linux lets clean file-cache folios grow with available memory and reclaims
/// them from its LRU under pressure. This smaller kernel still uses a bounded
/// block cache, but scaling that bound avoids treating an 8 GiB machine like
/// a fixed 8 MiB appliance. Entries are allocated on demand and remain
/// evictable; this function does not reserve the budget up front.
pub fn configure_block_cache_for_memory(memory_bytes: usize) -> usize {
    let capacity = capacity_for_memory(memory_bytes);
    #[cfg(not(test))]
    BLOCK_CACHE_CAPACITY.store(capacity, Ordering::Relaxed);
    capacity
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct CacheKey {
    block_id: usize,
    device_id: usize,
}

/// Deterministic mixer for internal block/device identifiers. Unlike a tree,
/// lookup cost stays approximately constant as the cache grows. These keys are
/// produced by validated filesystem mappings rather than attacker-provided
/// strings, so per-boot random seeding is unnecessary here.
#[derive(Default)]
struct CacheKeyHasher(u64);

impl CacheKeyHasher {
    fn mix(value: u64) -> u64 {
        let mut value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}

impl Hasher for CacheKeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = Self::mix(self.0 ^ u64::from(*byte));
        }
    }

    fn write_usize(&mut self, value: usize) {
        self.0 = Self::mix(self.0 ^ value as u64);
    }
}

type CacheKeyBuildHasher = BuildHasherDefault<CacheKeyHasher>;
type CacheEntryMap = HashMap<CacheKey, CacheSlot, CacheKeyBuildHasher>;

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
    evicting: bool,
    /// A newer queue record already represents hits since the last reclaim
    /// inspection. Further hits can be coalesced until that record is seen.
    promotion_pending: bool,
}

struct LoadState {
    done: AtomicBool,
}

impl LoadState {
    fn new() -> Self {
        Self {
            done: AtomicBool::new(false),
        }
    }

    fn finish(&self) {
        self.done.store(true, Ordering::Release);
    }

    fn is_done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }
}

struct LoadingEntry {
    state: Arc<LoadState>,
    block_device: Arc<dyn BlockDevice>,
}

enum CacheSlot {
    Loading(LoadingEntry),
    Ready(CacheEntry),
}

struct LoadTicket {
    start_block: usize,
    count: usize,
    state: Arc<LoadState>,
    block_device: Arc<dyn BlockDevice>,
    reserved_keys: Vec<CacheKey>,
}

struct EvictionTicket {
    key: CacheKey,
    stamp: u64,
    cache: Arc<Mutex<BlockCache>>,
    was_clean: bool,
}

enum CacheLookup {
    Ready(Arc<Mutex<BlockCache>>),
    Created(Arc<Mutex<BlockCache>>),
    Wait(Arc<LoadState>),
    Load(LoadTicket),
    Evict(EvictionTicket),
    Retry,
}

/// Block cache manager
pub struct BlockCacheManager {
    entries: CacheEntryMap,
    queue: VecDeque<(CacheKey, u64)>,
    stamp: u64,
}

impl BlockCacheManager {
    /// Create new block cache manager
    pub fn new() -> Self {
        Self {
            entries: HashMap::with_hasher(CacheKeyBuildHasher::default()),
            queue: VecDeque::new(),
            stamp: 0,
        }
    }

    fn next_stamp(&mut self) -> u64 {
        self.stamp = self.stamp.wrapping_add(1);
        self.stamp
    }

    fn insert_ready(&mut self, key: CacheKey, cache: Arc<Mutex<BlockCache>>) {
        let stamp = self.next_stamp();
        self.entries.insert(
            key,
            CacheSlot::Ready(CacheEntry {
                cache,
                stamp,
                evicting: false,
                promotion_pending: false,
            }),
        );
        self.queue.push_back((key, stamp));
    }

    fn lookup(&mut self, key: CacheKey) -> Option<CacheLookup> {
        let promotion_stamp = self.stamp.wrapping_add(1);
        let mut promotion = None;
        let found = match self.entries.get_mut(&key) {
            Some(CacheSlot::Ready(entry)) => {
                if entry.evicting {
                    // A lookup after isolation must cancel the eviction even
                    // if the caller drops its Arc before finish_eviction().
                    // Otherwise a racing write can happen after the eviction
                    // captured its writeback target and then be discarded when
                    // the transient strong reference returns to zero.
                    entry.evicting = false;
                    entry.stamp = promotion_stamp;
                    entry.promotion_pending = true;
                    promotion = Some((key, promotion_stamp));
                } else if !entry.promotion_pending {
                    entry.stamp = promotion_stamp;
                    entry.promotion_pending = true;
                    promotion = Some((key, promotion_stamp));
                }
                Some(CacheLookup::Ready(Arc::clone(&entry.cache)))
            }
            Some(CacheSlot::Loading(entry)) => Some(CacheLookup::Wait(Arc::clone(&entry.state))),
            None => None,
        };
        if let Some(candidate) = promotion {
            self.stamp = promotion_stamp;
            self.queue.push_back(candidate);
        }
        found
    }

    fn restore_scanned_to_front(&mut self, scanned: Vec<(CacheKey, u64)>) {
        for candidate in scanned.into_iter().rev() {
            self.queue.push_front(candidate);
        }
    }

    fn rotate_scanned_to_back(&mut self, scanned: Vec<(CacheKey, u64)>) {
        for candidate in scanned {
            self.queue.push_back(candidate);
        }
    }

    fn start_eviction(
        &mut self,
        key: CacheKey,
        stamp: u64,
        was_clean: bool,
    ) -> Option<EvictionTicket> {
        let entry = match self.entries.get_mut(&key) {
            Some(CacheSlot::Ready(entry))
                if entry.stamp == stamp
                    && !entry.evicting
                    && Arc::strong_count(&entry.cache) == 1 =>
            {
                entry
            }
            _ => return None,
        };
        entry.evicting = true;
        Some(EvictionTicket {
            key,
            stamp,
            cache: Arc::clone(&entry.cache),
            was_clean,
        })
    }

    fn select_eviction(&mut self) -> Option<EvictionTicket> {
        // Linux's reclaim scanner gives clean file-cache pages preference, but
        // bounds each scan and falls back to writeback instead of holding an
        // LRU lock while walking an unbounded dirty set. Keep the same
        // responsiveness property here: inspect a small window, prefer the
        // first clean candidate, otherwise evict its oldest eligible member.
        let scan_limit = self.queue.len().min(CLEAN_RECLAIM_SCAN_LIMIT);
        let mut scanned = Vec::with_capacity(scan_limit);
        let mut dirty_fallback = None;

        for _ in 0..scan_limit {
            let Some((key, stamp)) = self.queue.pop_front() else {
                break;
            };

            let mut is_current = false;
            let reclaimable_clean = match self.entries.get_mut(&key) {
                Some(CacheSlot::Ready(entry)) if entry.stamp == stamp => {
                    is_current = true;
                    entry.promotion_pending = false;
                    if !entry.evicting && Arc::strong_count(&entry.cache) == 1 {
                        entry.cache.try_lock().map(|cache| cache.is_clean())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if !is_current {
                // Discard stale LRU records.
                continue;
            }

            match reclaimable_clean {
                Some(true) => {
                    let ticket = self.start_eviction(key, stamp, true);
                    self.restore_scanned_to_front(scanned);
                    if ticket.is_some() {
                        return ticket;
                    }
                    self.queue.push_front((key, stamp));
                    return None;
                }
                Some(false) if dirty_fallback.is_none() => {
                    dirty_fallback = Some((key, stamp));
                    scanned.push((key, stamp));
                }
                _ => scanned.push((key, stamp)),
            }
        }

        if let Some((key, stamp)) = dirty_fallback {
            if let Some(index) = scanned
                .iter()
                .position(|candidate| *candidate == (key, stamp))
            {
                scanned.remove(index);
            }
            let ticket = self.start_eviction(key, stamp, false);
            self.restore_scanned_to_front(scanned);
            if ticket.is_some() {
                return ticket;
            }
            self.queue.push_front((key, stamp));
            return None;
        }

        // No reclaimable entry was found in this bounded window. Advance the
        // scan position instead of restoring the same records to the front:
        // otherwise a run of externally referenced or temporarily locked
        // entries can hide a reclaimable entry forever. This mirrors Linux's
        // bounded LRU walkers, whose cursor progresses between scan batches.
        self.rotate_scanned_to_back(scanned);
        None
    }

    fn finish_eviction(&mut self, ticket: &EvictionTicket) -> bool {
        let can_remove = matches!(
            self.entries.get(&ticket.key),
            Some(CacheSlot::Ready(entry))
                if entry.evicting
                    && entry.stamp == ticket.stamp
                    && Arc::ptr_eq(&entry.cache, &ticket.cache)
                    && Arc::strong_count(&entry.cache) == 2
        );
        if can_remove {
            self.entries.remove(&ticket.key);
            CACHE_ENTRIES.fetch_sub(1, Ordering::Relaxed);
            CACHE_EVICTIONS.fetch_add(1, Ordering::Relaxed);
            if ticket.was_clean {
                CACHE_CLEAN_EVICTIONS.fetch_add(1, Ordering::Relaxed);
            } else {
                CACHE_DIRTY_EVICTIONS.fetch_add(1, Ordering::Relaxed);
            }
            true
        } else {
            let mut requeue = None;
            if let Some(CacheSlot::Ready(entry)) = self.entries.get_mut(&ticket.key)
                && Arc::ptr_eq(&entry.cache, &ticket.cache)
            {
                entry.evicting = false;
                // A concurrent lookup already installed a newer queue record.
                // Only add one here when cancellation was caused by some other
                // transient reference (for example a sync-all snapshot).
                if !entry.promotion_pending {
                    requeue = Some(entry.stamp);
                }
            }
            if let Some(stamp) = requeue {
                self.queue.push_back((ticket.key, stamp));
            }
            false
        }
    }

    fn prepare_load(
        &mut self,
        block_device: Arc<dyn BlockDevice>,
        start_block: usize,
        count: usize,
    ) -> CacheLookup {
        let requested_key = cache_key(start_block, &block_device);
        if let Some(found) = self.lookup(requested_key) {
            return found;
        }

        let capacity = block_cache_capacity();
        let mut range_keys = Vec::with_capacity(count.min(capacity));
        for index in 0..count.max(1).min(capacity) {
            let Some(block_id) = start_block.checked_add(index) else {
                break;
            };
            range_keys.push(cache_key(block_id, &block_device));
        }
        let missing_keys = range_keys
            .iter()
            .copied()
            .filter(|key| !self.entries.contains_key(key))
            .collect::<Vec<_>>();

        if self.entries.len().saturating_add(missing_keys.len()) > capacity {
            return self
                .select_eviction()
                .map(CacheLookup::Evict)
                .unwrap_or(CacheLookup::Retry);
        }

        let state = Arc::new(LoadState::new());
        for key in missing_keys.iter().copied() {
            self.entries.insert(
                key,
                CacheSlot::Loading(LoadingEntry {
                    state: Arc::clone(&state),
                    block_device: Arc::clone(&block_device),
                }),
            );
        }
        CACHE_ENTRIES.fetch_add(missing_keys.len(), Ordering::Relaxed);

        CacheLookup::Load(LoadTicket {
            start_block,
            count: range_keys.len(),
            state,
            block_device,
            reserved_keys: missing_keys,
        })
    }

    fn publish_load(
        &mut self,
        ticket: &LoadTicket,
        mut loaded: Vec<(CacheKey, Arc<Mutex<BlockCache>>)>,
    ) -> Arc<Mutex<BlockCache>> {
        let requested_key = cache_key(ticket.start_block, &ticket.block_device);
        let mut published = 0u64;

        // Publish in reverse physical order so the explicitly requested block
        // receives the newest LRU stamp rather than the speculative tail.
        loaded.reverse();
        for (key, cache) in loaded {
            let owns_slot = matches!(
                self.entries.get(&key),
                Some(CacheSlot::Loading(entry))
                    if Arc::ptr_eq(&entry.state, &ticket.state)
            );
            if !owns_slot {
                continue;
            }
            self.insert_ready(key, cache);
            published = published.saturating_add(1);
        }

        let requested = match self.entries.get(&requested_key) {
            Some(CacheSlot::Ready(entry)) => Arc::clone(&entry.cache),
            _ => panic!("missing requested block cache after load"),
        };
        ticket.state.finish();
        CACHE_LOADS.fetch_add(1, Ordering::Relaxed);
        CACHE_PREFETCHED_BLOCKS.fetch_add(published.saturating_sub(1), Ordering::Relaxed);
        requested
    }

    fn prepare_zeroed(
        &mut self,
        block_id: usize,
        block_device: Arc<dyn BlockDevice>,
    ) -> CacheLookup {
        let key = cache_key(block_id, &block_device);
        if let Some(found) = self.lookup(key) {
            return found;
        }
        if self.entries.len() >= block_cache_capacity() {
            return self
                .select_eviction()
                .map(CacheLookup::Evict)
                .unwrap_or(CacheLookup::Retry);
        }
        let block_cache = Arc::new(Mutex::new(BlockCache::new_zeroed(block_id, block_device)));
        self.insert_ready(key, Arc::clone(&block_cache));
        CACHE_ENTRIES.fetch_add(1, Ordering::Relaxed);
        CacheLookup::Created(block_cache)
    }

    fn snapshots(
        &self,
    ) -> (
        Vec<Arc<Mutex<BlockCache>>>,
        Vec<(Arc<LoadState>, Arc<dyn BlockDevice>)>,
    ) {
        let mut ready = Vec::new();
        let mut loading = Vec::new();
        for slot in self.entries.values() {
            match slot {
                CacheSlot::Ready(entry) => ready.push(Arc::clone(&entry.cache)),
                CacheSlot::Loading(entry) => {
                    loading.push((Arc::clone(&entry.state), Arc::clone(&entry.block_device)))
                }
            }
        }
        (ready, loading)
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
    get_block_cache_with_hint(block_id, block_device, 1)
}

fn wait_for_load(state: &LoadState, block_device: &Arc<dyn BlockDevice>) {
    while !state.is_done() {
        CACHE_WAIT_RETRIES.fetch_add(1, Ordering::Relaxed);
        block_device.io_relax();
    }
}

fn sync_cache_through(cache: &Arc<Mutex<BlockCache>>, target: u64) {
    loop {
        let preparation = cache.lock().prepare_writeback(target);
        match preparation {
            WritebackPreparation::Done => return,
            WritebackPreparation::Busy(block_device) => block_device.io_relax(),
            WritebackPreparation::Ready(writeback) => {
                writeback
                    .block_device
                    .write_block(writeback.block_id, &writeback.data);
                cache.lock().finish_writeback(writeback.generation);
            }
        }
    }
}

pub(crate) fn sync_block_cache(cache: &Arc<Mutex<BlockCache>>) {
    let target = cache.lock().writeback_target();
    sync_cache_through(cache, target);
}

fn sync_eviction(ticket: EvictionTicket) {
    sync_block_cache(&ticket.cache);
    {
        let mut manager = BLOCK_CACHE_MANAGER.lock();
        manager.finish_eviction(&ticket);
    }
}

/// Get a block cache and, on a miss, load up to `read_ahead` contiguous blocks.
///
/// The caller must bound the hint to a known contiguous on-disk range. Existing
/// ready entries are never overwritten, so speculative reads cannot clobber a
/// dirty cache block.
pub(crate) fn get_block_cache_with_hint(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
    read_ahead: usize,
) -> Arc<Mutex<BlockCache>> {
    let mut miss_counted = false;
    loop {
        let action = {
            let mut manager = BLOCK_CACHE_MANAGER.lock();
            manager.prepare_load(Arc::clone(&block_device), block_id, read_ahead)
        };
        match action {
            CacheLookup::Ready(cache) => {
                if !miss_counted {
                    CACHE_HITS.fetch_add(1, Ordering::Relaxed);
                }
                return cache;
            }
            CacheLookup::Created(cache) => return cache,
            CacheLookup::Wait(state) => {
                if !miss_counted {
                    CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
                    CACHE_COALESCED_WAITS.fetch_add(1, Ordering::Relaxed);
                    miss_counted = true;
                }
                wait_for_load(&state, &block_device);
            }
            CacheLookup::Load(ticket) => {
                if !miss_counted {
                    CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
                }
                let mut buf = vec![0u8; ticket.count.saturating_mul(BLOCK_SZ)];
                ticket
                    .block_device
                    .read_blocks(ticket.start_block, &mut buf);

                let mut loaded = Vec::with_capacity(ticket.reserved_keys.len());
                for index in 0..ticket.count {
                    let block_id = ticket.start_block + index;
                    let key = cache_key(block_id, &ticket.block_device);
                    if !ticket.reserved_keys.contains(&key) {
                        continue;
                    }
                    let offset = index * BLOCK_SZ;
                    let cache = Arc::new(Mutex::new(BlockCache::new_with_data(
                        block_id,
                        Arc::clone(&ticket.block_device),
                        &buf[offset..offset + BLOCK_SZ],
                    )));
                    loaded.push((key, cache));
                }

                let cache = {
                    let mut manager = BLOCK_CACHE_MANAGER.lock();
                    manager.publish_load(&ticket, loaded)
                };
                return cache;
            }
            CacheLookup::Evict(ticket) => sync_eviction(ticket),
            CacheLookup::Retry => {
                CACHE_WAIT_RETRIES.fetch_add(1, Ordering::Relaxed);
                block_device.io_relax();
            }
        }
    }
}

/// Get a zero-initialized block cache for a freshly allocated block.
pub fn get_block_cache_zeroed(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
) -> Arc<Mutex<BlockCache>> {
    let mut miss_counted = false;
    loop {
        let action = {
            let mut manager = BLOCK_CACHE_MANAGER.lock();
            manager.prepare_zeroed(block_id, Arc::clone(&block_device))
        };
        match action {
            CacheLookup::Ready(cache) => {
                if !miss_counted {
                    CACHE_HITS.fetch_add(1, Ordering::Relaxed);
                }
                return cache;
            }
            CacheLookup::Created(cache) => {
                if !miss_counted {
                    CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
                }
                return cache;
            }
            CacheLookup::Wait(state) => {
                if !miss_counted {
                    CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
                    CACHE_COALESCED_WAITS.fetch_add(1, Ordering::Relaxed);
                    miss_counted = true;
                }
                wait_for_load(&state, &block_device);
            }
            CacheLookup::Evict(ticket) => sync_eviction(ticket),
            CacheLookup::Retry => {
                CACHE_WAIT_RETRIES.fetch_add(1, Ordering::Relaxed);
                block_device.io_relax();
            }
            CacheLookup::Load(_) => unreachable!("zeroed cache lookup cannot start a disk load"),
        }
    }
}

/// Sync all block caches to disk
pub fn block_cache_sync_all() {
    loop {
        let (ready, loading) = {
            let manager = BLOCK_CACHE_MANAGER.lock();
            manager.snapshots()
        };
        if !loading.is_empty() {
            for (state, block_device) in loading {
                wait_for_load(&state, &block_device);
            }
            continue;
        }
        let targets = ready
            .into_iter()
            .map(|cache| {
                let target = cache.lock().writeback_target();
                (cache, target)
            })
            .collect::<Vec<_>>();
        for (cache, target) in targets {
            sync_cache_through(&cache, target);
        }
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::collections::BTreeMap;
    use std::sync::{Condvar, Mutex as StdMutex, MutexGuard as StdMutexGuard, OnceLock, mpsc};
    use std::thread;
    use std::time::Duration;

    #[derive(Default)]
    struct GateState {
        target: Option<usize>,
        blocked: bool,
        entered: bool,
    }

    #[derive(Default)]
    struct IoGate {
        state: StdMutex<GateState>,
        changed: Condvar,
    }

    impl IoGate {
        fn block(&self, block_id: usize) {
            let mut state = self.state.lock().unwrap();
            state.target = Some(block_id);
            state.blocked = true;
            state.entered = false;
        }

        fn wait_if_blocked(&self, start_block: usize, count: usize) {
            let mut state = self.state.lock().unwrap();
            let end_block = start_block.saturating_add(count);
            let is_target = state
                .target
                .is_some_and(|target| start_block <= target && target < end_block);
            if !state.blocked || !is_target {
                return;
            }
            state.entered = true;
            self.changed.notify_all();
            while state.blocked {
                state = self.changed.wait(state).unwrap();
            }
        }

        fn wait_until_entered(&self) {
            let state = self.state.lock().unwrap();
            let (state, timeout) = self
                .changed
                .wait_timeout_while(state, Duration::from_secs(2), |state| !state.entered)
                .unwrap();
            assert!(
                !timeout.timed_out() && state.entered,
                "I/O gate was not entered"
            );
        }

        fn release(&self) {
            let mut state = self.state.lock().unwrap();
            state.blocked = false;
            self.changed.notify_all();
        }
    }

    #[derive(Default)]
    struct TestDevice {
        blocks: StdMutex<BTreeMap<usize, Vec<u8>>>,
        read_operations: AtomicUsize,
        blocks_read: AtomicUsize,
        write_operations: AtomicUsize,
        read_gate: IoGate,
        write_gate: IoGate,
    }

    impl TestDevice {
        fn disk_byte(block_id: usize) -> u8 {
            (block_id % 251) as u8
        }

        fn first_disk_byte(&self, block_id: usize) -> u8 {
            self.blocks
                .lock()
                .unwrap()
                .get(&block_id)
                .map_or_else(|| Self::disk_byte(block_id), |data| data[0])
        }
    }

    impl BlockDevice for TestDevice {
        fn io_relax(&self) {
            thread::yield_now();
        }

        fn read_block(&self, block_id: usize, buf: &mut [u8]) {
            self.read_blocks(block_id, buf);
        }

        fn write_block(&self, block_id: usize, buf: &[u8]) {
            self.write_blocks(block_id, buf);
        }

        fn read_blocks(&self, block_id: usize, buf: &mut [u8]) {
            assert_eq!(buf.len() % BLOCK_SZ, 0);
            let count = buf.len() / BLOCK_SZ;
            self.read_operations.fetch_add(1, Ordering::Relaxed);
            self.blocks_read.fetch_add(count, Ordering::Relaxed);
            self.read_gate.wait_if_blocked(block_id, count);
            let blocks = self.blocks.lock().unwrap();
            for (index, chunk) in buf.chunks_mut(BLOCK_SZ).enumerate() {
                let id = block_id + index;
                if let Some(data) = blocks.get(&id) {
                    chunk.copy_from_slice(data);
                } else {
                    chunk.fill(Self::disk_byte(id));
                }
            }
        }

        fn write_blocks(&self, block_id: usize, buf: &[u8]) {
            assert_eq!(buf.len() % BLOCK_SZ, 0);
            let count = buf.len() / BLOCK_SZ;
            self.write_operations.fetch_add(1, Ordering::Relaxed);
            self.write_gate.wait_if_blocked(block_id, count);
            let mut blocks = self.blocks.lock().unwrap();
            for (index, chunk) in buf.chunks(BLOCK_SZ).enumerate() {
                blocks.insert(block_id + index, chunk.to_vec());
            }
        }
    }

    fn test_lock() -> StdMutexGuard<'static, ()> {
        static TEST_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
        TEST_LOCK.get_or_init(|| StdMutex::new(())).lock().unwrap()
    }

    fn reset_cache() {
        {
            let mut manager = BLOCK_CACHE_MANAGER.lock();
            *manager = BlockCacheManager::new();
        }
        CACHE_HITS.store(0, Ordering::Relaxed);
        CACHE_MISSES.store(0, Ordering::Relaxed);
        CACHE_LOADS.store(0, Ordering::Relaxed);
        CACHE_COALESCED_WAITS.store(0, Ordering::Relaxed);
        CACHE_WAIT_RETRIES.store(0, Ordering::Relaxed);
        CACHE_EVICTIONS.store(0, Ordering::Relaxed);
        CACHE_CLEAN_EVICTIONS.store(0, Ordering::Relaxed);
        CACHE_DIRTY_EVICTIONS.store(0, Ordering::Relaxed);
        CACHE_PREFETCHED_BLOCKS.store(0, Ordering::Relaxed);
        CACHE_ENTRIES.store(0, Ordering::Relaxed);
    }

    #[test]
    fn diagnostics_does_not_wait_for_manager_lock() {
        let _test_guard = test_lock();
        reset_cache();
        let manager = BLOCK_CACHE_MANAGER.lock();
        let (sent, received) = mpsc::channel();
        let reader = thread::spawn(move || {
            sent.send(cache_diagnostics()).unwrap();
        });

        received
            .recv_timeout(Duration::from_millis(500))
            .expect("cache diagnostics waited for the global manager lock");
        drop(manager);
        reader.join().unwrap();
        reset_cache();
    }

    #[test]
    fn concurrent_cold_miss_is_single_flight() {
        let _test_guard = test_lock();
        reset_cache();
        let device = Arc::new(TestDevice::default());
        device.read_gate.block(7);

        let loader_device: Arc<dyn BlockDevice> = device.clone();
        let loader = thread::spawn(move || get_block_cache(7, loader_device));
        device.read_gate.wait_until_entered();

        let waiter_device: Arc<dyn BlockDevice> = device.clone();
        let waiter = thread::spawn(move || get_block_cache(7, waiter_device));
        while cache_diagnostics().coalesced_waits == 0 {
            thread::yield_now();
        }

        device.read_gate.release();
        let first = loader.join().unwrap();
        let second = waiter.join().unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(device.read_operations.load(Ordering::Relaxed), 1);
        assert_eq!(cache_diagnostics().loads, 1);
        drop((first, second));
        reset_cache();
    }

    #[test]
    fn blocked_cache_fill_does_not_hold_manager_lock() {
        let _test_guard = test_lock();
        reset_cache();
        let device = Arc::new(TestDevice::default());
        let block_device: Arc<dyn BlockDevice> = device.clone();
        drop(get_block_cache(1, Arc::clone(&block_device)));
        device.read_gate.block(9);

        let miss_device = Arc::clone(&block_device);
        let miss = thread::spawn(move || get_block_cache(9, miss_device));
        device.read_gate.wait_until_entered();

        let (sent, received) = mpsc::channel();
        let hit_device = Arc::clone(&block_device);
        let hit = thread::spawn(move || {
            let cache = get_block_cache(1, hit_device);
            sent.send(cache).unwrap();
        });
        let cached = received
            .recv_timeout(Duration::from_millis(500))
            .expect("cache hit blocked behind unrelated disk I/O");

        device.read_gate.release();
        drop(cached);
        hit.join().unwrap();
        drop(miss.join().unwrap());
        reset_cache();
    }

    #[test]
    fn sync_all_does_not_hold_manager_lock_during_writeback() {
        let _test_guard = test_lock();
        reset_cache();
        let device = Arc::new(TestDevice::default());
        let block_device: Arc<dyn BlockDevice> = device.clone();
        drop(get_block_cache(2, Arc::clone(&block_device)));
        drop(get_block_cache_zeroed(1, Arc::clone(&block_device)));
        device.write_gate.block(1);

        let sync = thread::spawn(block_cache_sync_all);
        device.write_gate.wait_until_entered();

        let (sent, received) = mpsc::channel();
        let hit_device = Arc::clone(&block_device);
        let hit = thread::spawn(move || {
            let cache = get_block_cache(2, hit_device);
            sent.send(cache).unwrap();
        });
        let cached = received
            .recv_timeout(Duration::from_millis(500))
            .expect("cache lookup blocked behind writeback");

        device.write_gate.release();
        drop(cached);
        hit.join().unwrap();
        sync.join().unwrap();
        reset_cache();
    }

    #[test]
    fn writeback_releases_cache_lock_and_preserves_racing_write() {
        let _test_guard = test_lock();
        reset_cache();
        let device = Arc::new(TestDevice::default());
        let block_device: Arc<dyn BlockDevice> = device.clone();
        let cache = get_block_cache_zeroed(1, Arc::clone(&block_device));
        cache.lock().get_bytes_mut(0, 1)[0] = 0xaa;
        device.write_gate.block(1);

        let sync = thread::spawn(block_cache_sync_all);
        device.write_gate.wait_until_entered();

        let (sent, received) = mpsc::channel();
        let writer_cache = Arc::clone(&cache);
        let writer = thread::spawn(move || {
            writer_cache.lock().get_bytes_mut(0, 1)[0] = 0xbb;
            sent.send(()).unwrap();
        });
        received
            .recv_timeout(Duration::from_millis(500))
            .expect("cache writer blocked behind device I/O");

        device.write_gate.release();
        writer.join().unwrap();
        sync.join().unwrap();
        assert_eq!(device.first_disk_byte(1), 0xaa);

        block_cache_sync_all();
        assert_eq!(device.first_disk_byte(1), 0xbb);
        assert_eq!(device.write_operations.load(Ordering::Relaxed), 2);
        drop(cache);
        reset_cache();
    }

    #[test]
    fn read_ahead_batches_io_without_clobbering_dirty_cache() {
        let _test_guard = test_lock();
        reset_cache();
        let device = Arc::new(TestDevice::default());
        let block_device: Arc<dyn BlockDevice> = device.clone();
        let dirty = get_block_cache_zeroed(11, Arc::clone(&block_device));
        dirty.lock().get_bytes_mut(0, 1)[0] = 0xaa;

        let requested = get_block_cache_with_hint(10, Arc::clone(&block_device), 3);
        assert_eq!(
            requested.lock().get_bytes(0, 1)[0],
            TestDevice::disk_byte(10)
        );
        let same_dirty = get_block_cache(11, Arc::clone(&block_device));
        assert!(Arc::ptr_eq(&dirty, &same_dirty));
        assert_eq!(same_dirty.lock().get_bytes(0, 1)[0], 0xaa);
        drop(get_block_cache(12, Arc::clone(&block_device)));

        assert_eq!(device.read_operations.load(Ordering::Relaxed), 1);
        assert_eq!(device.blocks_read.load(Ordering::Relaxed), 3);
        assert_eq!(cache_diagnostics().prefetched_blocks, 1);
        drop((dirty, same_dirty, requested));
        reset_cache();
    }

    #[test]
    fn coalesced_promotions_preserve_lru_order() {
        let _test_guard = test_lock();
        reset_cache();
        let device = Arc::new(TestDevice::default());
        let block_device: Arc<dyn BlockDevice> = device.clone();

        let mut block_one = None;
        for block_id in 0..block_cache_capacity() {
            let cache = get_block_cache(block_id, Arc::clone(&block_device));
            if block_id == 1 {
                block_one = Some(Arc::downgrade(&cache));
            }
            drop(cache);
        }
        for _ in 0..(block_cache_capacity() * 10) {
            drop(get_block_cache(0, Arc::clone(&block_device)));
        }
        assert_eq!(
            BLOCK_CACHE_MANAGER.lock().queue.len(),
            block_cache_capacity() + 1
        );

        drop(get_block_cache(
            block_cache_capacity(),
            Arc::clone(&block_device),
        ));
        assert!(
            block_one.unwrap().upgrade().is_none(),
            "queue compaction changed LRU order instead of evicting block 1"
        );
        reset_cache();
    }

    #[test]
    fn dirty_reclaim_scan_is_bounded_and_keeps_lru_order() {
        let _test_guard = test_lock();
        let device = Arc::new(TestDevice::default());
        let block_device: Arc<dyn BlockDevice> = device;
        let mut manager = BlockCacheManager::new();
        let entry_count = CLEAN_RECLAIM_SCAN_LIMIT + 8;

        for block_id in 0..entry_count {
            let key = cache_key(block_id, &block_device);
            let stamp = manager.next_stamp();
            manager.entries.insert(
                key,
                CacheSlot::Ready(CacheEntry {
                    cache: Arc::new(Mutex::new(BlockCache::new_zeroed(
                        block_id,
                        Arc::clone(&block_device),
                    ))),
                    stamp,
                    evicting: false,
                    promotion_pending: false,
                }),
            );
            manager.queue.push_back((key, stamp));
        }

        let ticket = manager
            .select_eviction()
            .expect("dirty cache should provide a reclaim fallback");
        assert_eq!(ticket.key.block_id, 0);
        assert_eq!(manager.queue.len(), entry_count - 1);
        assert_eq!(manager.queue.front().map(|(key, _)| key.block_id), Some(1));
    }

    #[test]
    fn bounded_reclaim_scan_advances_past_busy_window() {
        let _test_guard = test_lock();
        let device = Arc::new(TestDevice::default());
        let block_device: Arc<dyn BlockDevice> = device;
        let mut manager = BlockCacheManager::new();
        let entry_count = CLEAN_RECLAIM_SCAN_LIMIT + 1;
        let mut busy = Vec::with_capacity(CLEAN_RECLAIM_SCAN_LIMIT);

        for block_id in 0..entry_count {
            let key = cache_key(block_id, &block_device);
            let cache = Arc::new(Mutex::new(BlockCache::new(
                block_id,
                Arc::clone(&block_device),
            )));
            if block_id < CLEAN_RECLAIM_SCAN_LIMIT {
                busy.push(Arc::clone(&cache));
            }
            manager.insert_ready(key, cache);
        }

        assert!(
            manager.select_eviction().is_none(),
            "the first bounded window contains only externally referenced entries"
        );
        let ticket = manager
            .select_eviction()
            .expect("the next scan must advance to the reclaimable entry");
        assert_eq!(ticket.key.block_id, CLEAN_RECLAIM_SCAN_LIMIT);
        drop(busy);
    }

    #[test]
    fn lookup_cancels_eviction_before_racing_write_is_dropped() {
        let _test_guard = test_lock();
        let device = Arc::new(TestDevice::default());
        let block_device: Arc<dyn BlockDevice> = device.clone();
        let mut manager = BlockCacheManager::new();
        let key = cache_key(7, &block_device);
        manager.insert_ready(
            key,
            Arc::new(Mutex::new(BlockCache::new(
                key.block_id,
                Arc::clone(&block_device),
            ))),
        );

        let ticket = manager
            .select_eviction()
            .expect("the clean cache should be isolated for eviction");
        // Model an eviction that has already captured and completed its old
        // clean writeback target before another lookup modifies the block.
        sync_block_cache(&ticket.cache);

        let cache = match manager.lookup(key) {
            Some(CacheLookup::Ready(cache)) => cache,
            _ => panic!("isolated cache must remain visible to a racing lookup"),
        };
        cache.lock().get_bytes_mut(0, 1)[0] = 0x5a;
        drop(cache);

        assert!(
            !manager.finish_eviction(&ticket),
            "a racing lookup must cancel eviction after its Arc is dropped"
        );
        assert_eq!(manager.queue.len(), 1, "cancellation queued twice");

        let cache = match manager.lookup(key) {
            Some(CacheLookup::Ready(cache)) => cache,
            _ => panic!("cancelled eviction removed the cache entry"),
        };
        assert_eq!(cache.lock().get_bytes(0, 1)[0], 0x5a);
        sync_block_cache(&cache);
        assert_eq!(device.first_disk_byte(key.block_id), 0x5a);
    }

    #[test]
    fn clean_reclaim_precedes_dirty_writeback() {
        let _test_guard = test_lock();
        reset_cache();
        let device = Arc::new(TestDevice::default());
        let block_device: Arc<dyn BlockDevice> = device.clone();
        let dirty = get_block_cache_zeroed(0, Arc::clone(&block_device));
        dirty.lock().get_bytes_mut(0, 1)[0] = 0x5a;
        drop(dirty);

        for block_id in 1..block_cache_capacity() {
            drop(get_block_cache(block_id, Arc::clone(&block_device)));
        }
        drop(get_block_cache(
            block_cache_capacity(),
            Arc::clone(&block_device),
        ));

        assert_eq!(device.write_operations.load(Ordering::Relaxed), 0);
        let dirty = get_block_cache(0, Arc::clone(&block_device));
        assert_eq!(dirty.lock().get_bytes(0, 1)[0], 0x5a);
        let diagnostics = cache_diagnostics();
        assert_eq!(diagnostics.evictions, 1);
        assert_eq!(diagnostics.clean_evictions, 1);
        assert_eq!(diagnostics.dirty_evictions, 0);
        drop(dirty);
        reset_cache();
    }

    #[test]
    fn dirty_lru_eviction_writes_back_when_no_clean_candidate_exists() {
        let _test_guard = test_lock();
        reset_cache();
        let device = Arc::new(TestDevice::default());
        let block_device: Arc<dyn BlockDevice> = device.clone();

        for block_id in 0..block_cache_capacity() {
            let dirty = get_block_cache_zeroed(block_id, Arc::clone(&block_device));
            if block_id == 0 {
                dirty.lock().get_bytes_mut(0, 1)[0] = 0x5a;
            }
            drop(dirty);
        }
        drop(get_block_cache(
            block_cache_capacity(),
            Arc::clone(&block_device),
        ));

        assert_eq!(device.first_disk_byte(0), 0x5a);
        assert_eq!(device.write_operations.load(Ordering::Relaxed), 1);
        let diagnostics = cache_diagnostics();
        assert_eq!(diagnostics.evictions, 1);
        assert_eq!(diagnostics.clean_evictions, 0);
        assert_eq!(diagnostics.dirty_evictions, 1);
        reset_cache();
    }

    #[test]
    fn memory_scaled_capacity_is_bounded() {
        assert_eq!(capacity_for_memory(0), DEFAULT_BLOCK_CACHE_CAPACITY);
        assert_eq!(
            capacity_for_memory(8usize * 1024 * 1024 * 1024),
            MAX_BLOCK_CACHE_CAPACITY
        );
        assert_eq!(capacity_for_memory(usize::MAX), MAX_BLOCK_CACHE_CAPACITY);
    }
}

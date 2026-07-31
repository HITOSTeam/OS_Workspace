//! Host harness for the architecture-independent VFS and tmpfs tests.
//!
//! The real source files are compiled directly.  Only kernel services that are
//! irrelevant to object semantics are replaced with small host definitions.

extern crate alloc;

pub mod config {
    pub const PAGE_SIZE: usize = 4096;
}

pub mod mm {
    use alloc::vec::Vec;

    pub struct UserBuffer {
        pub buffers: Vec<&'static mut [u8]>,
    }
}

pub mod time {
    use core::sync::atomic::{AtomicU64, Ordering};

    static NOW: AtomicU64 = AtomicU64::new(1);

    pub fn get_time_ns() -> u64 {
        NOW.fetch_add(1, Ordering::Relaxed)
    }
}

pub mod fs {
    use crate::mm::UserBuffer;
    use core::any::Any;

    pub const POLLIN: i16 = 1;
    pub const POLLOUT: i16 = 4;

    pub trait File: Send + Sync {
        fn readable(&self) -> bool;
        fn writable(&self) -> bool;
        fn read(&self, buffer: UserBuffer) -> usize;
        fn write(&self, buffer: UserBuffer) -> usize;
        fn fixed_poll_mask(&self) -> Option<i16> {
            None
        }
        fn as_any(&self) -> &dyn Any;
    }

    #[path = "../../../../../os/src/fs/vfs/mod.rs"]
    pub mod vfs;

    #[path = "../../../../../os/src/fs/tmpfs/mod.rs"]
    pub mod tmpfs;
}

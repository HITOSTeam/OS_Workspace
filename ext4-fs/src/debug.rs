//! Debug logging support for ext4-fs

/// Logger trait for debug output
pub trait Logger: Sync {
    fn log(&self, record: &str);
}

static mut LOGGER: Option<&'static dyn Logger> = None;

/// Set the global logger
pub fn set_logger(logger: &'static dyn Logger) {
    unsafe {
        LOGGER = Some(logger);
    }
}

/// Log a debug message
pub fn debug_log(msg: &str) {
    unsafe {
        if let Some(logger) = LOGGER {
            logger.log(msg);
        }
    }
}

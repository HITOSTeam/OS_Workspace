// src/log.rs
pub trait Logger {
    fn log(&self, msg: &str);
}

static mut LOGGER: Option<&'static dyn Logger> = None;

pub fn set_logger(logger: &'static dyn Logger) {
    unsafe { LOGGER = Some(logger) };
}

pub fn debug_log(msg: &str) {
    unsafe {
        if let Some(l) = LOGGER {
            l.log(msg);
        }
    }
}

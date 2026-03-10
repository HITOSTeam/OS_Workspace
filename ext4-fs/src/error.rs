#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ext4Error {
    NotADirectory,
    NotAFile,
    AlreadyExists,
    NotFound,
    NoSpace,
    NameTooLong,
    Unsupported,
    InvalidInput,
}

pub type Result<T> = core::result::Result<T, Ext4Error>;

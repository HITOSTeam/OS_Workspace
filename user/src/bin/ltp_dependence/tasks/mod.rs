//! LTP task-list catalog, split by subsystem.
//!
//! Each submodule is a bag of `pub const *_TASKS: [&str; N]` literals that
//! drive [`super::submit_plan`]. Items are grouped by the syscall family
//! they exercise; the dated campaign batches live in [`batches`] and the
//! still-unwired "next push" lists in [`backlog`].
//!
//! Many groups are intentionally reachable but not yet consumed by
//! `submit_plan.rs`, so we silence the `unused_imports` lint at the module
//! level rather than per `pub use` line.

#![allow(unused_imports)]

mod backlog;
mod batches;
mod cred;
mod fs_io;
mod fs_meta;
mod ident;
mod process;
mod resource;
mod smoke;
mod time;
mod unrun;

pub use backlog::*;
pub use batches::*;
pub use cred::*;
pub use fs_io::*;
pub use fs_meta::*;
pub use ident::*;
pub use process::*;
pub use resource::*;
pub use smoke::*;
pub use time::*;
pub use unrun::*;

//! LTP dependency facade.
//!
//! The test-list catalog lives in [`tasks`] (split by subsystem), and the
//! scheduler that picks which groups to run for each architecture lives in
//! [`submit_plan`]. This file is intentionally tiny: it wires the two
//! together and keeps the catalog reachable for plan entries.

mod submit_plan;
mod tasks;

pub use submit_plan::{run_non_riscv_ltp_groups, run_riscv_ltp_groups};
#[allow(unused_imports)]
pub use tasks::*;

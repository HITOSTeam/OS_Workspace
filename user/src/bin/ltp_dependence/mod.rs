//! LTP dependency facade.
//!
//! The test-list catalog lives in [`tasks`] (split by subsystem), and the
//! scheduler that picks which batches to run for each architecture lives
//! in [`submit_plan`]. This file is intentionally tiny: it just wires the
//! two together and re-exports the catalog so `submit_plan.rs` can keep
//! saying `use super::*;`.

mod submit_plan;
mod tasks;

pub use submit_plan::{run_non_riscv_ltp_groups, run_riscv_ltp_groups};
pub use tasks::*;

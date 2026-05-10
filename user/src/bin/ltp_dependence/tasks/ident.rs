//! System identity: UTS (hostname/domainname) plus getrandom.
//
// Auto-extracted from the original flat `mod.rs`; test names are
// preserved verbatim so the set consumed by `submit_plan.rs` is unchanged.

#![allow(dead_code)]

// UTS nodename/domainname setters.
pub const UTS_NAME_TASKS: [&str; 6] = [
    "sethostname01",
    "sethostname02",
    "sethostname03",
    "setdomainname01",
    "setdomainname02",
    "setdomainname03",
];
pub const UTS_QUERY_TASKS: [&str; 2] = ["gethostname01", "getdomainname01"];
// Special case: gethostname02 expects glibc behavior (ENAMETOOLONG on
// truncation). The shipped musl wrapper truncates and returns success.
// Run it explicitly in glibc-only lanes from submit_plan.
pub const GETRANDOM_TASKS: [&str; 5] = [
    "getrandom01",
    "getrandom02",
    "getrandom03",
    "getrandom04",
    "getrandom05",
];

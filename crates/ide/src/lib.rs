//! Fergents project layer (`spec.md` §1, task 3.1).
//!
//! A **project** is a code workspace, distinct from a brain. The kernel owns
//! its records, trust state, and file index. A newly opened project is
//! **untrusted**: nothing executable may run in it until the user trusts it
//! (spec decision D3).

mod error;
mod index;
mod model;
mod path;
mod service;

pub use error::ProjectError;
pub use model::{Project, SearchHit, TrustState};
pub use service::ProjectService;

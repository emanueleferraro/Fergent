//! Fergents brain model.
//!
//! A brain is a folder on disk with a `brain.toml` (identity + configuration)
//! and a `vault/` of markdown files, plus a regenerable `index.sqlite` and an
//! optional `graph.db` (`BRAIN.md` §2, `ARCHITECTURE.md` §8). This crate owns
//! `brain.toml`, the vault layout, the brain lifecycle, and multi-brain
//! operations.

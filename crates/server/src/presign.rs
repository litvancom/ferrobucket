//! Presigned URL generation for ferrobucket.
//!
//! The implementation has been moved to `crates/storage::presign` so that both
//! `crates/server` and `crates/ui` can use it without a crate cycle.
//! This module re-exports `presign_url` for backward compatibility with the
//! existing `presign` CLI subcommand and tests.

pub use ferrobucket_storage::presign::presign_url;

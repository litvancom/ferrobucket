//! UI handlers for the axum integration layer (Plan 03).
//!
//! Provides the two streaming axum handlers that the browser talks to directly:
//! - `download_handler` — streams object bytes from FsStorage with Content-Disposition
//! - `upload_handler` — small-file PUT + multipart part upload + complete/abort
//!
//! The `AppState` type used by these handlers is sourced from `ferrobucket_ui`
//! (defined in Plan 02 in `crates/ui/src/server_fns/state.rs`).

pub mod download;
pub mod upload;

pub use download::download_handler;
pub use upload::upload_handler;

/// Re-export AppState from the ui crate so the mount seam in main.rs
/// only needs to import from `crate::ui`.
pub use ferrobucket_ui::server_fns::state::AppState;

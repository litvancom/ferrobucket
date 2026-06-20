//! Leptos server functions (#[server]) for the web UI.
//!
//! All storage access goes through these server-side functions — never in WASM
//! (DEC-ui-ssr). Each submodule compiles only when the `ssr` feature is active
//! (the `#[server]` macro gates the function body to `ssr`).
//!
//! Submodules:
//! - `state`   — `AppState` holding `Arc<FsStorage>` + config (no secret serialized)
//! - `buckets` — list / create / delete bucket server fns
//! - `objects` — list / head / delete object server fns
//! - `presign` — presign URL minting (server-side SigV4, 900s TTL)
//! - `settings`— read-only connection config + live writable-status check

pub mod state;
pub mod buckets;
pub mod objects;
pub mod presign;
pub mod settings;

#[cfg(feature = "ssr")]
pub use state::AppState;

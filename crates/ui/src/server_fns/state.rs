//! `AppState` — shared server state injected as Leptos context for server
//! functions and as Axum `State` for the upload/download handlers (Plan 03).
//!
//! Why `Arc<FsStorage>`: `FsStorage` holds a `PathBuf` and is `Send + Sync`
//! but does not implement `Clone`. Wrapping in `Arc` makes `AppState` cheaply
//! cloneable, satisfying `leptos_routes_with_context`'s requirement that the
//! context closure be `Clone + 'static` (Pitfall 2 from RESEARCH.md).
//!
//! SECURITY (T-04-03, DEC-ui-ssr): `secret_key` is held here for SigV4
//! presigning but is NEVER serialized and NEVER returned from any server fn.
//! It is only accessed inside `#[server]` function bodies (ssr-only code).

#[cfg(feature = "ssr")]
mod inner {
    use std::sync::Arc;
    use axum::extract::FromRef;
    use leptos::config::LeptosOptions;
    use ferrobucket_storage::FsStorage;

    /// Shared application state injected into every Leptos server function via
    /// `provide_context` / `expect_context::<AppState>()`.
    ///
    /// All fields are `Clone`-able. `FsStorage` is wrapped in `Arc` because it
    /// is `Send + Sync` but not `Clone`.
    #[derive(Clone)]
    pub struct AppState {
        /// In-process storage backend. All server fn data access goes here (D-03).
        pub storage: Arc<FsStorage>,

        /// Leptos configuration — needed by `FromRef<AppState> for LeptosOptions`
        /// (required by `leptos_routes_with_context`).
        pub leptos_options: LeptosOptions,

        /// S3 endpoint URL (e.g. `"http://127.0.0.1:9000"`).
        /// Returned by `get_config_fn` in settings.rs (read-only, D-10).
        pub endpoint: String,

        /// S3 region string (e.g. `"us-east-1"`).
        pub region: String,

        /// Access key ID (the public identifier, NOT the secret).
        /// Returned by `get_config_fn`; the secret is held separately below.
        pub access_key_id: Option<String>,

        /// AWS secret access key — used only for SigV4 presigning (D-05).
        ///
        /// NEVER serialized. NEVER returned from any server fn or DTO.
        /// Only `presign_fn` in presign.rs accesses this field, server-side.
        pub secret_key: String,

        /// Filesystem path to the data directory (shown in Settings, D-10).
        pub data_root: std::path::PathBuf,

        /// Whether the server was started in anonymous mode.
        pub anonymous: bool,
    }

    /// Required by `leptos_routes_with_context` — the Leptos router extracts
    /// `LeptosOptions` from `AppState` via this impl to set up SSR context.
    ///
    /// [CITED: leptos_axum 0.8.9 `LeptosRoutes` trait — requires `FromRef<S>
    /// for LeptosOptions` on the state type `S`]
    impl FromRef<AppState> for LeptosOptions {
        fn from_ref(state: &AppState) -> Self {
            state.leptos_options.clone()
        }
    }
}

#[cfg(feature = "ssr")]
pub use inner::AppState;

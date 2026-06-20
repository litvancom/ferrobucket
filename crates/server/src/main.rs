//! ferrobucket — S3-compatible object storage server.
// Leptos SSR generates deeply nested generic types in release builds.
// The default limit (128) is insufficient; 256 resolves the overflow.
#![recursion_limit = "256"]
//!
//! Subcommands (D-01):
//!   - `serve` — run the S3-compatible HTTP server (original behaviour)
//!   - `presign <verb> <bucket> <key>` — generate a presigned URL
//!
//! Security: --secret-key is never printed or logged (T-03-07).

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::{get, post};
use clap::{Args, Parser, Subcommand};
use ferrobucket_storage::FsStorage;
use ferrobucket_server::{ui, FerrobucketS3};
use leptos::config::get_configuration;
use leptos::prelude::provide_context;
use leptos_axum::{generate_route_list, LeptosRoutes};
use tower_http::services::ServeDir;

// ─── Top-level CLI ────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "ferrobucket", about = "S3-compatible object storage")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the S3-compatible HTTP server.
    Serve(ServeArgs),
    /// Generate a presigned URL for a bucket object.
    Presign(PresignArgs),
}

// ─── `serve` subcommand ───────────────────────────────────────────────────────

/// Arguments for `ferrobucket serve`.
///
/// All fields are identical to the previous flat `Cli` struct so that existing
/// invocations continue to work under the `serve` subcommand.
///
/// Security: --secret-key is never logged or printed.
#[derive(Args, Debug)]
struct ServeArgs {
    /// Root directory for stored data.
    #[arg(long, default_value = "./data", env = "FERROBUCKET_DATA")]
    data: std::path::PathBuf,

    /// Address and port to listen on.
    #[arg(long, default_value = "127.0.0.1:9000", env = "FERROBUCKET_LISTEN")]
    listen: SocketAddr,

    /// AWS access key ID (required unless --anonymous).
    #[arg(long, env = "FERROBUCKET_ACCESS_KEY")]
    access_key: Option<String>,

    /// AWS secret access key (required unless --anonymous).
    /// Never logged or printed.
    #[arg(long, env = "FERROBUCKET_SECRET_KEY")]
    secret_key: Option<String>,

    /// S3 region reported to clients (any region accepted; D-06).
    #[arg(long, default_value = "us-east-1", env = "FERROBUCKET_REGION")]
    region: String,

    /// Skip SigV4 authentication entirely (dev/testing only; disables all auth).
    #[arg(long, default_value_t = false, env = "FERROBUCKET_ANONYMOUS")]
    anonymous: bool,
}

// ─── `presign` subcommand ─────────────────────────────────────────────────────

/// Arguments for `ferrobucket presign`.
///
/// Generates a presigned URL for the given verb/bucket/key combination.
/// Default TTL is 900 seconds (D-02); override with `--expires-in`.
/// Credentials can be supplied via flags or the standard AWS env vars.
#[derive(Args, Debug)]
struct PresignArgs {
    /// HTTP verb to presign (get, put, head, delete — D-03).
    #[arg(value_enum)]
    verb: PresignVerb,

    /// S3 bucket name.
    bucket: String,

    /// S3 object key.
    key: String,

    /// Endpoint host:port of the S3 server.
    #[arg(long, default_value = "127.0.0.1:9000")]
    endpoint: String,

    /// URL validity in seconds (D-02: default 900 = 15 minutes).
    #[arg(long, default_value_t = 900)]
    expires_in: u32,

    /// AWS access key ID.
    #[arg(long, env = "AWS_ACCESS_KEY_ID")]
    access_key: String,

    /// AWS secret access key. Never printed.
    #[arg(long, env = "AWS_SECRET_ACCESS_KEY")]
    secret_key: String,

    /// AWS region string.
    #[arg(long, default_value = "us-east-1")]
    region: String,
}

/// S3 verbs supported for presigned URL generation (D-03: all four verbs).
#[derive(clap::ValueEnum, Clone, Debug)]
enum PresignVerb {
    Get,
    Put,
    Head,
    Delete,
}

// ─── Error / tracing helpers ─────────────────────────────────────────────────

/// Error handler for HTTP-level failures (connection errors, internal service errors).
///
/// Note: S3 application-level errors (NoSuchBucket, 403, etc.) are already serialized
/// to proper HTTP responses by s3s and never reach this handler.
async fn handle_s3_error(err: s3s::HttpError) -> axum::response::Response {
    use axum::response::IntoResponse;
    tracing_or_stderr(&format!("s3 service error: {err:?}"));
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
}

/// Minimal tracing that works with or without the `tracing` crate.
/// We don't add `tracing` as a dep to keep the binary lean; errors go to stderr.
fn tracing_or_stderr(msg: &str) {
    eprintln!("{msg}");
}

// ─── Embedded asset module (embed-assets feature only) ───────────────────────

/// Serves cargo-leptos-built frontend assets embedded into the binary at compile time.
///
/// Only compiled when the `embed-assets` Cargo feature is active (release build via
/// `cargo leptos build --release`). The `#[folder]` path is relative to `crates/server/`,
/// resolving to workspace `target/site/pkg` — the directory cargo-leptos populates during
/// its frontend compile step (Pitfall 2: relative to crate, not workspace root).
///
/// In dev builds (feature absent) the `/pkg` route is handled by `tower_http::ServeDir`
/// reading from `target/site/pkg` on disk — unchanged from the pre-embedding path.
#[cfg(feature = "embed-assets")]
mod embedded {
    use axum::http::{StatusCode, header};
    use axum::response::{IntoResponse, Response};
    use rust_embed::Embed;

    #[derive(Embed)]
    #[folder = "../../target/site/pkg"]
    pub struct EmbeddedPkg;

    pub async fn pkg_handler(
        axum::extract::Path(path): axum::extract::Path<String>,
    ) -> impl IntoResponse {
        match EmbeddedPkg::get(&path) {
            Some(content) => {
                let mime = mime_guess::from_path(&path).first_or_octet_stream();
                ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
            }
            None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
        }
    }
}

// ─── Subcommand implementations ───────────────────────────────────────────────

/// Run the S3 HTTP server (original behaviour relocated from `main`).
async fn run_serve(args: ServeArgs) -> anyhow::Result<()> {
    // ── Build FsStorage (shared between S3 adapter and AppState) ────────────────
    // A single FsStorage is created; the S3 adapter takes ownership of one copy
    // and AppState wraps another in Arc. Both point to the same data root (args.data).
    // FsStorage only holds a PathBuf so constructing two instances is equivalent to
    // sharing one — no in-memory state divergence (FsStorage is stateless beyond the path).
    let storage_for_s3 = FsStorage::new(&args.data);
    let storage_for_ui = Arc::new(FsStorage::new(&args.data));

    // ── S3 service ───────────────────────────────────────────────────────────────
    let adapter = FerrobucketS3::new(storage_for_s3);
    let mut builder = s3s::service::S3ServiceBuilder::new(adapter);

    // D-07: conditional auth — --anonymous skips SimpleAuth entirely.
    if !args.anonymous {
        let access = args.access_key.clone().ok_or_else(|| {
            anyhow::anyhow!("--access-key is required unless --anonymous is set")
        })?;
        let secret = args.secret_key.clone().ok_or_else(|| {
            anyhow::anyhow!("--secret-key is required unless --anonymous is set")
        })?;
        // SimpleAuth is region-agnostic by design (D-06): it verifies the HMAC using
        // the region the client signed with; no server-side region is configured.
        builder.set_auth(s3s::auth::SimpleAuth::from_single(access, secret));
    }

    let s3_service = builder.build();

    // HandleError converts S3Service::Error (HttpError) -> Infallible for axum.
    // S3 application errors (NoSuchBucket, 403, etc.) never reach handle_s3_error;
    // they are already serialized to HTTP by s3s.
    let s3_wrapped = axum::error_handling::HandleError::new(s3_service, handle_s3_error);

    // ── Leptos configuration (D-03 cfg-gated split) ─────────────────────────────
    // In embedded mode: build LeptosOptions directly from code — no LEPTOS_* env,
    // no Cargo.toml, no get_configuration. The binary is self-contained and runs
    // from any directory (REQ-nfr-single-binary).
    // In dev mode: preserve the original get_configuration path so `cargo leptos watch`
    // and `cargo run` from the workspace root continue to work with hot-reload.
    #[cfg(feature = "embed-assets")]
    let leptos_options = leptos::config::LeptosOptions::builder()
        .output_name("ferrobucket-ui")
        .build();

    #[cfg(not(feature = "embed-assets"))]
    let leptos_options = {
        // `get_configuration(None)` reads from LEPTOS_* env vars set by `cargo leptos`.
        // For standalone `cargo run` from the workspace root, fall back to reading the
        // workspace Cargo.toml directly so `site_root` resolves correctly (Pitfall 5).
        get_configuration(None)
            .or_else(|_| get_configuration(Some("Cargo.toml")))
            .expect("could not load Leptos config (run from workspace root or use `cargo leptos`)")
            .leptos_options
    };

    // ── Build AppState ────────────────────────────────────────────────────────────
    let endpoint = format!("http://{}", args.listen);
    let app_state = ui::AppState {
        storage: storage_for_ui,
        leptos_options: leptos_options.clone(),
        endpoint,
        region: args.region.clone(),
        access_key_id: args.access_key.clone(),
        secret_key: args.secret_key.clone().unwrap_or_default(),
        data_root: args.data.clone(),
        anonymous: args.anonymous,
    };

    // ── Leptos route list ─────────────────────────────────────────────────────────
    let routes = generate_route_list(ferrobucket_ui::App);

    // ── Mount order (D-01, RESEARCH Pattern 1) ───────────────────────────────────
    //
    // Mount-order invariant: Leptos routes + /pkg + /ui handlers MUST appear BEFORE
    // `.fallback_service(s3_wrapped)` so that /ui/* and /pkg/* are never forwarded
    // to S3. The S3 fallback only handles requests that don't match earlier routes.
    //
    // Pitfall 1: do NOT use axum::Router::nest("/ui", ...) — the /ui prefix lives
    //            in the Leptos route tree (App's ParentRoute). Nesting creates /ui/ui/.
    //
    // /pkg routing: in embed-assets mode assets are served from binary memory via
    // embedded::pkg_handler; in dev mode ServeDir reads from target/site/pkg on disk.

    #[cfg(feature = "embed-assets")]
    let app = axum::Router::new()
        // 1. Upload/download axum handlers (raw streaming — no Leptos involvement)
        .route("/ui/upload/{bucket}/{*key}", post(ui::upload_handler))
        .route("/ui/download/{bucket}/{*key}", get(ui::download_handler))
        // 2. Embedded WASM/JS/CSS assets baked into the binary at compile time (D-01/D-02)
        .route("/pkg/{*path}", get(embedded::pkg_handler))
        // 3. Leptos SSR routes — injects AppState as Leptos context for server fns
        .leptos_routes_with_context(
            &app_state,
            routes,
            {
                let app_state = app_state.clone();
                move || provide_context(app_state.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || ferrobucket_ui::shell(leptos_options.clone())
            },
        )
        // 4. S3 fallback — MUST be last; never sees /ui/* or /pkg/* (mount-order invariant D-01)
        .fallback_service(s3_wrapped)
        .with_state(app_state);

    // In dev mode (no embed-assets): serve /pkg from disk via ServeDir for hot-reload.
    // Pitfall 5: derive path from site_root, not a raw CWD string.
    #[cfg(not(feature = "embed-assets"))]
    let app = axum::Router::new()
        // 1. Upload/download axum handlers (raw streaming — no Leptos involvement)
        .route("/ui/upload/{bucket}/{*key}", post(ui::upload_handler))
        .route("/ui/download/{bucket}/{*key}", get(ui::download_handler))
        // 2. Static WASM/JS/CSS assets built by cargo-leptos at target/site/pkg
        .nest_service("/pkg", ServeDir::new(&format!("{}/pkg", leptos_options.site_root)))
        // 3. Leptos SSR routes — injects AppState as Leptos context for server fns
        //    generate_route_list(App) emits paths starting with /ui (from ParentRoute),
        //    so the S3 fallback never sees /ui/* requests (RESEARCH A2, Pitfall 1).
        .leptos_routes_with_context(
            &app_state,
            routes,
            {
                let app_state = app_state.clone();
                move || provide_context(app_state.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || ferrobucket_ui::shell(leptos_options.clone())
            },
        )
        // 4. S3 fallback — MUST be last; never sees /ui/* or /pkg/* (mount-order invariant D-01)
        .fallback_service(s3_wrapped)
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Generate and print a presigned URL (synchronous; no server needed).
///
/// Maps the PresignVerb enum to its HTTP method string, builds the
/// path-style path, calls presign_url, and prints the result to stdout.
fn run_presign(args: PresignArgs) -> anyhow::Result<()> {
    let method = match args.verb {
        PresignVerb::Get => "GET",
        PresignVerb::Put => "PUT",
        PresignVerb::Head => "HEAD",
        PresignVerb::Delete => "DELETE",
    };
    // Path-style: /<bucket>/<key>
    let path = format!("/{}/{}", args.bucket, args.key);

    let url = ferrobucket_server::presign::presign_url(
        method,
        &args.endpoint,
        &path,
        args.expires_in, // D-02: default 900s
        &args.access_key,
        &args.secret_key,
        &args.region,
    );

    println!("{url}");
    Ok(())
}

// ─── Entry point ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Commands::Serve(args) => run_serve(args).await,
        Commands::Presign(args) => run_presign(args),
    }
}

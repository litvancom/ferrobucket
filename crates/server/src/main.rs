//! ferrobucket — S3-compatible object storage server.
//!
//! Subcommands (D-01):
//!   - `serve` — run the S3-compatible HTTP server (original behaviour)
//!   - `presign <verb> <bucket> <key>` — generate a presigned URL
//!
//! Security: --secret-key is never printed or logged (T-03-07).

use clap::{Args, Parser, Subcommand};
use ferrobucket_storage::FsStorage;
use ferrobucket_server::FerrobucketS3;
use std::net::SocketAddr;

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
    #[arg(long, default_value = "./data")]
    data: std::path::PathBuf,

    /// Address and port to listen on.
    #[arg(long, default_value = "127.0.0.1:9000")]
    listen: SocketAddr,

    /// AWS access key ID (required unless --anonymous).
    #[arg(long)]
    access_key: Option<String>,

    /// AWS secret access key (required unless --anonymous).
    /// Never logged or printed.
    #[arg(long)]
    secret_key: Option<String>,

    /// S3 region reported to clients (any region accepted; D-06).
    #[arg(long, default_value = "us-east-1")]
    region: String,

    /// Skip SigV4 authentication entirely (dev/testing only; disables all auth).
    #[arg(long, default_value_t = false)]
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

// ─── Subcommand implementations ───────────────────────────────────────────────

/// Run the S3 HTTP server (original behaviour relocated from `main`).
async fn run_serve(args: ServeArgs) -> anyhow::Result<()> {
    let storage = FsStorage::new(&args.data);
    let adapter = FerrobucketS3::new(storage);

    let mut builder = s3s::service::S3ServiceBuilder::new(adapter);

    // D-07: conditional auth — --anonymous skips SimpleAuth entirely.
    if !args.anonymous {
        let access = args.access_key.ok_or_else(|| {
            anyhow::anyhow!("--access-key is required unless --anonymous is set")
        })?;
        let secret = args.secret_key.ok_or_else(|| {
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

    // Phase 4 adds Leptos routes before .fallback_service() — zero lines here change.
    let app = axum::Router::new().fallback_service(s3_wrapped);

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

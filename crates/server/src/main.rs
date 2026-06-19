use clap::Parser;
use ferrobucket_storage::FsStorage;
use ferrobucket_server::FerrobucketS3;
use std::net::SocketAddr;

/// S3-compatible object storage server.
///
/// Security: --secret-key is never printed or logged.
#[derive(Parser, Debug)]
#[command(name = "ferrobucket", about = "S3-compatible object storage")]
struct Cli {
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let storage = FsStorage::new(&cli.data);
    let adapter = FerrobucketS3::new(storage);

    let mut builder = s3s::service::S3ServiceBuilder::new(adapter);

    // D-07: conditional auth — --anonymous skips SimpleAuth entirely.
    if !cli.anonymous {
        let access = cli.access_key.ok_or_else(|| {
            anyhow::anyhow!("--access-key is required unless --anonymous is set")
        })?;
        let secret = cli.secret_key.ok_or_else(|| {
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

    let listener = tokio::net::TcpListener::bind(cli.listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

# ferrobucket

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org/)
[![PRs welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)

A lightweight, self-hosted S3-compatible object storage server with a built-in web UI,
written in Rust and shipped as a single self-contained binary.

One line: run it, point `aws` at it, use it. No Docker required, no separate asset files,
no MinIO-sized RAM footprint.

---

## Contents

- [Quickstart](#quickstart)
- [S3 client configuration (path-style)](#s3-client-configuration-path-style)
- [Docker](#docker)
- [AWS deviations](#aws-deviations)
- [Memory footprint](#memory-footprint)
- [License](#license)

---

## Quickstart

### Build

```bash
# Prerequisites: Rust toolchain + cargo-leptos + wasm32 target
cargo install --locked cargo-leptos@0.3.6
rustup target add wasm32-unknown-unknown

# Build the release binary (frontend WASM first, then the embedded server binary)
cargo leptos build --release
```

The resulting binary is at `target/release/ferrobucket`.
All UI assets (WASM, JS, CSS) are embedded at compile time — the binary runs from
any directory with no external files required.

### Run

```bash
./target/release/ferrobucket serve \
  --data ./data \
  --access-key myaccesskey \
  --secret-key mysecretkey

# With --anonymous to skip SigV4 (dev/local use only):
./target/release/ferrobucket serve --data ./data --anonymous
```

All flags accept environment variables with the `FERROBUCKET_` prefix:

| Flag | Env var | Default |
|------|---------|---------|
| `--data` | `FERROBUCKET_DATA` | `./data` |
| `--listen` | `FERROBUCKET_LISTEN` | `127.0.0.1:9000` |
| `--access-key` | `FERROBUCKET_ACCESS_KEY` | — |
| `--secret-key` | `FERROBUCKET_SECRET_KEY` | — |
| `--region` | `FERROBUCKET_REGION` | `us-east-1` |
| `--anonymous` | `FERROBUCKET_ANONYMOUS` | `false` |

Once running:

- **S3 API** — `http://127.0.0.1:9000` (path-style; see next section)
- **Web UI** — `http://127.0.0.1:9000/ui`

---

## S3 client configuration (path-style)

ferrobucket uses **path-style addressing only**. Virtual-hosted-style URLs
(`bucket.host:9000`) are not supported. Configure every client to force path-style.

### aws CLI

```bash
aws --endpoint-url http://127.0.0.1:9000 \
    s3 mb s3://my-bucket

aws --endpoint-url http://127.0.0.1:9000 \
    s3 cp ./file.txt s3://my-bucket/file.txt

aws --endpoint-url http://127.0.0.1:9000 \
    s3 ls s3://my-bucket
```

The region value does not matter — ferrobucket accepts any region string.

### aws4fetch / SDK

```js
const client = new S3Client({
  endpoint: "http://127.0.0.1:9000",
  region: "us-east-1",            // any string
  forcePathStyle: true,           // required
  credentials: {
    accessKeyId: "myaccesskey",
    secretAccessKey: "mysecretkey",
  },
});
```

For AWS SDK for Rust, set `force_path_style(true)` on the config builder.
For any other SDK, the equivalent setting is `force_path_style = true`.

---

## Docker

### Single-arch (current host platform)

```bash
docker build -t ferrobucket:latest .

docker run \
  -e FERROBUCKET_ACCESS_KEY=myaccesskey \
  -e FERROBUCKET_SECRET_KEY=mysecretkey \
  -p 9000:9000 \
  -v ./data:/data \
  ferrobucket:latest
```

The container default listen address is `0.0.0.0:9000` (overridden by the Dockerfile
`ENV FERROBUCKET_LISTEN`); the published port is immediately reachable.

Using an env file:

```bash
# .env (keep out of version control)
FERROBUCKET_ACCESS_KEY=myaccesskey
FERROBUCKET_SECRET_KEY=mysecretkey

docker run --env-file .env -p 9000:9000 -v ./data:/data ferrobucket:latest
```

### Multi-arch (amd64 + arm64)

```bash
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t ferrobucket:latest .
```

The Dockerfile selects `x86_64-unknown-linux-musl` or `aarch64-unknown-linux-musl`
automatically via `ARG TARGETARCH`. The runtime image is based on `scratch` (~40 MB total).

---

## AWS deviations

ferrobucket implements the S3 wire protocol for the common subset of operations.
The following are intentional deviations from AWS S3 behaviour.

### 1. Multipart upload ETag format

AWS S3 computes multipart ETags as the MD5 of the concatenated part-MD5s, producing
`"<md5-of-md5s>-<N>"`. ferrobucket stores each part's MD5 during upload and produces
`"<md5hex>-<N>"` instead — a stable, deterministic value that differs from the AWS
format. Single-part (PutObject) ETags are identical to AWS (`MD5 hex of the body`).

If your application validates multipart ETags against the AWS formula, the values will
not match.

### 2. Reserved bucket names: `ui` and `pkg`

The bucket names `ui` and `pkg` are reserved by ferrobucket for the built-in web UI
(`/ui`) and the embedded frontend assets (`/pkg`). Attempting to create a bucket with
either name returns an error. All other valid S3 bucket names are accepted.

### 3. No ACL, IAM, tagging, or CORS configuration; single static credential

ferrobucket authenticates via a single static access-key/secret-key pair (SigV4).
There is no multi-user IAM, no ACL semantics, no bucket/object tagging, and no
CORS-configuration API. All authenticated requests from the single credential have
full access. The `--anonymous` flag disables even the SigV4 check for local dev use.

### 4. `ListParts` and `ListMultipartUploads` return NotImplemented

The `ListParts` and `ListMultipartUploads` operations are not implemented in v1
and return an S3 `NotImplemented` error. Multipart upload, complete, and abort work
correctly; only the listing APIs are absent.

### Additional v1 scope boundaries

The following features are explicitly out of scope for v1 and will return errors or
be silently absent: object versioning, Object Lock / WORM, lifecycle policies,
replication, server-side encryption (SSE/KMS), S3 Select.

---

## Memory footprint

**Measured idle RSS: ~6 MB**

| Server | Idle RSS |
|--------|----------|
| ferrobucket | ~6 MB |
| MinIO (published) | 200–300 MB |

### Measurement

```bash
# Start with an empty data directory
./target/release/ferrobucket serve --data /tmp/empty --anonymous &
SERVER_PID=$!

# Wait for startup to stabilise
sleep 5

# macOS
ps -o pid,rss= -p $SERVER_PID
# rss is in KB; divide by 1024 to get MB

# Linux
grep VmRSS /proc/$SERVER_PID/status

kill $SERVER_PID
```

**Measurement conditions:**

- Host: macOS 15 (Darwin 24.6.0), Apple Silicon (aarch64)
- Binary: release build with `embed-assets` feature (`cargo leptos build --release`)
- Data directory: empty (no buckets, no objects)
- State: post-startup steady state after 5 seconds
- Load: no active requests
- Result: **5,952 KB (~6 MB RSS)** (`ps -o rss=`)

MinIO's 200–300 MB figure is from MinIO's own published documentation and benchmarks.
ferrobucket was measured on a different host and workload — this is not a head-to-head
benchmark, but the order-of-magnitude difference reflects the design intent:
a minimal Tokio/Axum server with embedded WASM vs. a full distributed storage system.

---

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for build/test
instructions and project conventions. For security issues, please follow
[SECURITY.md](SECURITY.md) — do not open a public issue.

## License

ferrobucket is licensed under the **MIT License**.

See the [`LICENSE`](LICENSE) file for the full license text.

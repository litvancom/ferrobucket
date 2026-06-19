# docs/DECISIONS.md — ferrobucket Phase 0 Verified Facts

> **Purpose:** Single source of truth for external-crate API facts verified by compile-proof spikes.
> Every fact in this document cites its source (crate version, docs.rs URL, or source file path)
> so a future reader can distinguish a verified fact from a recalled guess (D-04).
>
> **Authoritative version pins (D-06):** The exact `=x.y.z` versions that actually compiled in the
> Phase 0 spikes. Phase 1 inherits these pins unchanged.
>
> | Crate            | Authoritative Pin | Spike that verified it                    |
> |------------------|-------------------|-------------------------------------------|
> | `s3s`            | `=0.13.0`         | `scratch/s3s-spike/` — `cargo build` exit 0 (2026-06-19) |
> | `async-trait`    | `=0.1.89`         | resolved by s3s workspace; used in spike  |
> | `leptos`         | `=0.8.19`         | `scratch/leptos-spike/` — `cargo leptos build` exit 0 (2026-06-19) |
> | `leptos_axum`    | `=0.8.9`          | resolved in Cargo.lock; compiled 2026-06-19 |
> | `leptos_meta`    | `=0.8.6`          | compiled 2026-06-19 (independent versioning — NOT 0.8.19) |
> | `leptos_router`  | `=0.8.13`         | compiled 2026-06-19 (independent versioning — NOT 0.8.19) |
> | `axum`           | `=0.8.8`          | resolved by leptos_axum; compiled 2026-06-19 |
> | `cargo-leptos`   | `0.3.6` (CLI)     | `cargo-leptos --version` 2026-06-19       |
>
> **Throwaway spikes:** `scratch/s3s-spike/` and `scratch/leptos-spike/` are gitignored (D-02) and
> can be deleted after this phase. Only this document and the committed state/summary files persist.

---

## 1. s3s

*See §2 for s3s-fs reference wiring patterns.*

### 1.1 Pinned Version

| Property | Value | Source |
|----------|-------|--------|
| **Pinned version** | `0.13.0` | `crates.io/api/v1/crates/s3s/versions` queried 2026-06-19; `cargo build` in `scratch/s3s-spike/` exits 0 |
| **Published** | 2026-03-01 | crates.io registry |
| **Latest pre-release (excluded)** | `0.14.0-alpha.1` | excluded per D-06 (pre-release) |
| **Minimum Rust** | `1.92.0` stable | `github.com/Nugine/s3s v0.13.0/Cargo.toml` `rust-version` field |
| **Rust edition** | 2024 | `github.com/Nugine/s3s v0.13.0/Cargo.toml` `edition` field |
| **Required dep** | `async-trait = "0.1.89"` | s3s workspace pins; compile-confirmed in `scratch/s3s-spike/Cargo.toml` |

### 1.2 Correct Import Paths (s3s 0.13.0)

**Compile-discovered (wrong paths caused compiler errors; correct paths confirmed from `src/lib.rs`):**

```rust
// All re-exported from the crate root:
use s3s::{S3, S3Request, S3Response, S3Result, s3_error};
use s3s::dto::*;                          // all XInput / XOutput types
use s3s::auth::SimpleAuth;                // single static credential
use s3s::service::S3ServiceBuilder;       // builds S3Service
```

**Source:** `s3s-0.13.0/src/lib.rs` lines 160–169 (public re-exports) + compile proof in `scratch/s3s-spike/src/main.rs` (2026-06-19).

**Note:** Sub-modules `s3s::request`, `s3s::response` do NOT exist in the public API. `s3s::error` is private. All four names (`S3Request`, `S3Response`, `S3Result`, `s3_error`) are re-exported at the crate root.

### 1.3 S3 Trait — Method Signature Shape

**Compile-verified source:** `scratch/s3s-spike/src/main.rs` + `cargo build` exit 0 (2026-06-19)

Every method in the `S3` trait follows this exact shape:

```rust
#[async_trait::async_trait]
impl S3 for YourStorage {
    async fn method_name(
        &self,
        req: S3Request<XInput>,
    ) -> S3Result<S3Response<XOutput>> {
        // ...
    }
}
```

**Key rules (compile-verified):**

- `#[async_trait::async_trait]` is **required** on the `impl` block — s3s 0.13.0 uses async-trait desugaring, NOT RPITIT/`impl Future`.
- `S3Result<T>` is a type alias for `Result<T, S3Error>`.
- All ~150 trait methods have **default impls** returning `Err(s3_error!(NotImplemented))` — only override the methods you implement; the rest are covered by defaults.
- The trait bound for `S3ServiceBuilder::new(storage)` is `T: S3 + Clone + Send + Sync + 'static`.

**Source:** `docs.rs/s3s/0.13.0/s3s/trait.S3.html` + compile proof in `scratch/s3s-spike/src/main.rs`

### 1.4 S3Request Struct Fields

**Compile-verified source:** `s3s-0.13.0/src/protocol.rs` (inspected from cargo registry cache)

```rust
pub struct S3Request<T> {
    pub input: T,                                    // S3 operation input (XInput)
    pub method: http::Method,
    pub uri: http::Uri,
    pub headers: http::HeaderMap,
    pub extensions: http::Extensions,
    pub credentials: Option<Credentials>,            // None = anonymous request
    pub region: Option<Region>,
    pub service: Option<String>,
    pub trailing_headers: Option<TrailingHeaders>,   // SigV4 streaming trailers
}
```

**Input access patterns (compile-verified in `scratch/s3s-spike/src/main.rs`):**

```rust
// Field access:
let _bucket = req.input.bucket;   // CreateBucketInput, GetObjectInput

// Destructuring:
let PutObjectInput { bucket, key, body, content_type, .. } = req.input;
// body: Option<s3s::Body>  (stream handle; must be Some for a valid upload)
```

**Source:** `s3s-0.13.0/src/protocol.rs` struct definition + destructuring compile proof (2026-06-19)

### 1.5 Presigned URL Generation — EXPLICITLY ABSENT (Phase 3 Decision Point)

**Compile-verified by absence:** No `presign` module in `s3s-0.13.0/src/` directory listing; no `presign` re-export in `s3s-0.13.0/src/lib.rs` lines 129–170 (inspected 2026-06-19).

**What s3s DOES provide:**
- Incoming presigned request **verification** — `X-Amz-Signature` in query params is validated automatically by the s3s auth chain. No application code is needed to verify presigned requests from clients.

**What s3s does NOT provide:**
- Any API to **generate** presigned URLs for clients to use.

**Phase 3 decision required:** ferrobucket must implement presigned GET/PUT URL generation independently. Options (not resolved in Phase 0):
- (a) Hand-rolled query-string SigV4 (~100 lines of HMAC-SHA256 signing)
- (b) `aws-sigv4` crate (AWS-maintained, lower-level)
- (c) `aws-sdk-s3` presign client (higher-level but larger dependency)

**Source:** `s3s-0.13.0/src/lib.rs` module list (no `presign` present) + `s3s-0.13.0/src/` directory listing (2026-06-19)

### 1.6 S3Service Axum Mounting — NOT a Router (Phase 2 Decision Point)

`S3Service` (produced by `S3ServiceBuilder::build()`) is a **tower/hyper service**, not an `axum::Router`.

**What this means:**
- `Router::merge(s3_service)` and `Router::nest("/", s3_service)` will **fail** — `S3Service` does not implement `axum::handler::Handler` or `Into<Router>`.
- `S3Service` implements `tower::Service<http::Request<B>>` and `hyper::service::Service`.

**Phase 2 integration approach (reference-only, not compile-verified in Phase 0):** Use `axum`'s tower interop — `Router::fallback_service(s3_service)` or `Router::route_service(...)`. Alternatively, keep `hyper_util` as the top-level server (as in the s3s-fs reference).

**Source:** `github.com/Nugine/s3s crates/s3s-fs/src/main.rs` (uses `hyper_util`, not Axum Router) + RESEARCH.md §Pitfall 2 (2026-06-19)

---

## 2. s3s-fs Patterns (REFERENCE-ONLY)

> **REFERENCE-ONLY — NOT a shipped dependency.**
> Per `DEC-own-storage-backend` (PROJECT.md): ferrobucket ships its own filesystem backend.
> `s3s-fs` is used only as a reference for wiring patterns. Do NOT add `s3s-fs` to any shipped `Cargo.toml`.

### 2.1 Backend Wiring Pattern

**Source:** `github.com/Nugine/s3s crates/s3s-fs/src/main.rs` + compile proof of `S3ServiceBuilder` at `scratch/s3s-spike/src/main.rs` (2026-06-19)

```rust
use s3s::auth::SimpleAuth;
use s3s::service::S3ServiceBuilder;

// Build the service (T: S3 + Clone + Send + Sync + 'static)
let mut b = S3ServiceBuilder::new(YourStorage);
b.set_auth(SimpleAuth::from_single("ACCESS_KEY", "SECRET_KEY"));
let service = b.build();   // → S3Service (implements Clone + tower::Service + hyper::Service)
```

### 2.2 Reference Serve Loop (NOT Axum)

**Source:** `github.com/Nugine/s3s crates/s3s-fs/src/main.rs`

```rust
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use tokio::net::TcpListener;

let listener = TcpListener::bind("127.0.0.1:9000").await?;
let http_server = ConnBuilder::new(TokioExecutor::new());
loop {
    let (socket, _) = listener.accept().await?;
    let conn = http_server.serve_connection(TokioIo::new(socket), service.clone());
    tokio::spawn(async move { let _ = conn.await; });
}
```

`S3Service` is `Clone` — the `service.clone()` per-connection pattern is the intended usage.

### 2.3 Input Field Destructuring (reference from s3s-fs/src/s3.rs)

**Source:** `github.com/Nugine/s3s crates/s3s-fs/src/s3.rs` (read 2026-06-19)

```rust
// PutObject — body is Option<s3s::Body>
let PutObjectInput { body, bucket, key, content_type, .. } = req.input;
let body = body.ok_or_else(|| s3_error!(InvalidRequest, "missing body"))?;

// GetObject — field access
let _bucket = req.input.bucket;
let _key = req.input.key;
```

---

## 3. Leptos / cargo-leptos

### 3.1 Pinned Versions (D-06)

**Compile-verified source:** `scratch/leptos-spike/` — `cargo leptos build` exit 0 (2026-06-19)

| Crate | Pinned Version | Version Notes |
|-------|---------------|---------------|
| `leptos` | `=0.8.19` | Latest stable; `0.9.0-alpha` excluded (D-06) |
| `leptos_axum` | `=0.8.9` | Resolved in Cargo.lock; matches leptos 0.8.x |
| `leptos_meta` | `=0.8.6` | **Independent versioning** — latest stable 0.8.x is NOT 0.8.19 |
| `leptos_router` | `=0.8.13` | **Independent versioning** — latest stable 0.8.x is NOT 0.8.19 |
| `axum` | `=0.8.8` | Compatible with leptos_axum 0.8.9 |
| `cargo-leptos` (CLI) | `0.3.6` | `cargo-leptos --version` on dev machine (2026-06-19) |
| `wasm-bindgen` | `0.2.x` | Resolved at build time (0.2.125 used); wasm32-unknown-unknown target already installed |

**Critical note on independent versioning:** `leptos_meta` and `leptos_router` publish independently from the `leptos` crate. Do NOT assume they share the same patch version as `leptos`. The `=0.8.19` pin applies only to `leptos` itself. Pinning `leptos_meta = "=0.8.19"` will fail (`cargo leptos build` error: "failed to select a version for the requirement leptos_meta = '=0.8.19'" — candidate versions are 0.9.0-alpha, 0.8.6, 0.8.5, ...).

**Source:** crates.io registry + `scratch/leptos-spike/Cargo.toml` compile-verified (2026-06-19)

### 3.2 Required `[lib]` Section

**Compile-verified source:** `scratch/leptos-spike/Cargo.toml` (cargo leptos build exit 0, 2026-06-19)

```toml
[lib]
crate-type = ["cdylib", "rlib"]
```

Both crate types are required:
- `cdylib` — for WASM (browser hydration target)
- `rlib` — for native linking (SSR server binary)

### 3.3 Confirmed `ssr`/`hydrate` Feature Contents

**Compile-verified source:** `scratch/leptos-spike/Cargo.toml` (cargo leptos build exit 0, 2026-06-19)

```toml
[features]
hydrate = [
    "leptos/hydrate",
    "dep:wasm-bindgen",
]
ssr = [
    "dep:axum",
    "dep:tokio",
    "dep:leptos_axum",
    "leptos/ssr",
    "leptos_meta/ssr",
    "leptos_router/ssr",
]
```

### 3.4 Required `[package.metadata.leptos]` Keys

**Compile-verified source:** `scratch/leptos-spike/Cargo.toml` (cargo leptos build exit 0, 2026-06-19)

```toml
[package.metadata.leptos]
output-name = "ferrobucket-ui"
site-root = "target/site"
site-pkg-dir = "pkg"
lib-features = ["hydrate"]
bin-features = ["ssr"]
```

**Required minimum keys:** `output-name`, `site-root`, `site-pkg-dir`, `lib-features`, `bin-features`.

**Important:** `style-file` MUST be omitted if no SCSS/CSS file exists. Its presence triggers npm/sass dependency. Absence is safe.

### 3.5 Source Code Patterns (compile-verified)

**Source:** `scratch/leptos-spike/src/` (cargo leptos build exit 0, 2026-06-19)

**`src/app.rs`** — Shared component (compiled for both SSR and WASM targets):
```rust
use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    view! { <p>"hello"</p> }
}
```

**`src/lib.rs`** — WASM/hydrate entry (compiled to `wasm32-unknown-unknown`):
```rust
pub mod app;
pub use app::App;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn hydrate() {
    leptos::mount::hydrate_body(App);
}
```

**`src/main.rs`** — SSR server entry (native only):
```rust
#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    // leptos_axum wires routes via Router::leptos_routes(...)
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}   // empty stub for WASM compilation
```

### 3.6 Build Commands

**Source:** `scratch/leptos-spike/` — compile-verified (2026-06-19)

- **`cargo leptos build`** — compiles lib crate to `wasm32-unknown-unknown` with `--features hydrate`, and bin crate natively with `--features ssr`.
- **`cargo leptos build --release`** — release mode.
- **Do NOT use `cargo build`** — it skips the WASM path and does not prove the hydrate compilation.

### 3.7 Workspace Form (Phase 4 — NOT compile-verified in Phase 0)

For a multi-crate workspace, `[package.metadata.leptos]` becomes `[[workspace.metadata.leptos]]` in the root `Cargo.toml`:

```toml
[[workspace.metadata.leptos]]
name = "ferrobucket-ui"
bin-package = "ferrobucket-server"
lib-package = "ferrobucket-ui"
site-root = "target/site"
lib-features = ["hydrate"]
bin-features = ["ssr"]
output-name = "ferrobucket-ui"
```

**Source:** RESEARCH.md §Q4 (from leptos-rs/start-axum template + Leptos book) — **not** compile-verified in Phase 0; Phase 4 will verify.

---

## 4. Name availability

> Checked at plan execution time (2026-06-19) per D-05. Results recorded with the exact command run and
> the check date — each entry is its own source citation (D-04). Re-verify before any publish in Phase 5.

| Check | Command | Observed Response | Result | Date |
|-------|---------|-------------------|--------|------|
| crates.io crate `ferrobucket` | `curl -s -A "ferrobucket-phase0-check/1.0" "https://crates.io/api/v1/crates/ferrobucket"` | `{"errors":[{"detail":"crate \`ferrobucket\` does not exist"}]}` | **AVAILABLE** | 2026-06-19 |
| GitHub repo `ferrobucket/ferrobucket` | `curl -s "https://api.github.com/repos/ferrobucket/ferrobucket"` | `{"message": "Not Found", "status": "404"}` | **AVAILABLE** | 2026-06-19 |
| GitHub org `ferrobucket` | `curl -s "https://api.github.com/orgs/ferrobucket"` | `{"message": "Not Found", ...}` | **AVAILABLE** | 2026-06-19 |
| GitHub user `ferrobucket` | `curl -s "https://api.github.com/users/ferrobucket"` | `{"message": "Not Found", ...}` | **AVAILABLE** | 2026-06-19 |

**Verdict: `ferrobucket` is AVAILABLE on crates.io and GitHub (all four checks).**

No blocker. No fallback name was chosen (D-05). No placeholder crate was published or reserved (D-05).

**Note:** Initial crates.io query without a `User-Agent` header returned a policy-violation error (`"We are unable to process your request at this time"`). A second query with a descriptive `User-Agent` header returned the definitive `"crate \`ferrobucket\` does not exist"` response confirming availability.

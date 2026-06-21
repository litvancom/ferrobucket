# Contributing to ferrobucket

Thanks for your interest in contributing! ferrobucket is a lightweight, self-hosted,
S3-compatible object storage server with a built-in web UI, written entirely in Rust.
It targets local development and homelab use — **not** production/distributed storage.

By participating you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md).

## Getting started

### Prerequisites

- Rust (stable) with the `wasm32-unknown-unknown` target
- [`cargo-leptos`](https://github.com/leptos-rs/cargo-leptos) `0.3.6`

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked cargo-leptos@0.3.6
```

### Build & run

```bash
# Backend + storage (no UI toolchain needed):
cargo build
cargo test

# Storage crate only (pure logic, no HTTP):
cargo test -p ferrobucket-storage

# Full app (SSR + WASM UI) — built with cargo-leptos, NOT cargo run:
cargo leptos watch -- serve --anonymous --data ./data --listen 127.0.0.1:3000
#   → S3 API + UI at http://127.0.0.1:3000  (UI at /ui)

# Release binary (embedded assets, runs from any directory):
cargo leptos build --release
```

See the [README](README.md) for client configuration and runtime flags.

## Before you open a pull request

- `cargo fmt --all` — formatting must be clean.
- `cargo clippy --workspace --all-targets` — no new warnings.
- `cargo test` (and `cargo leptos build` if you touched the UI) must pass.
- Keep changes focused; one logical change per PR.

## Project invariants (please don't silently reverse)

These are deliberate architecture decisions (see [`docs/DECISIONS.md`](docs/DECISIONS.md)):

- **S3 protocol via the `s3s` crate.** Don't hand-roll S3 parsing/XML/SigV4 on raw Axum.
- **Keep `ferrobucket-storage` decoupled.** It must not import `s3s` types. Translation
  between `s3s` types and the internal `Storage` trait lives only in the server crate.
- **The UI is server-side rendered.** Credentials and request signing stay server-side —
  never move signing or secret material into the browser/WASM path.
- **Object keys may contain `/`.** "Folders" are derived from prefixes + delimiter at
  listing time; they are never created as real directories.

## Conventions

- Match the surrounding code style; keep functions small and well-named.
- Prefer the standard library over new dependencies; justify any new crate in the PR.
- Any new deviation from AWS S3 behaviour **must be documented in the README**.
- Integration tests drive the running server with the real `aws` CLI (path-style).
- Record significant design decisions in `docs/DECISIONS.md`.

## Reporting bugs / requesting features

Use the GitHub issue templates. For security issues, **do not** open a public issue —
see [SECURITY.md](SECURITY.md).

## License

By contributing, you agree that your contributions will be licensed under the
[MIT License](LICENSE).

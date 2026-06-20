# ── Stage 1: Build ────────────────────────────────────────────────────────────
# Uses rust:alpine for musl-native builds (D-04: static musl → scratch runtime).
# If the musl path fails, see the documented debian:slim glibc fallback below.
FROM rust:alpine AS builder

# Install build dependencies:
#   musl-dev   — musl libc headers + musl-gcc linker (required for x86_64-unknown-linux-musl)
#   binaryen   — provides wasm-opt (cargo-leptos uses this to optimise the WASM binary)
#   npm        — required by cargo-leptos for any JS tooling steps
#   curl       — used below to download the pre-built cargo-leptos binary
RUN apk add --no-cache musl-dev binaryen npm curl

# Install cargo-leptos@0.3.6 from the official pre-built musl binary (leptos-rs/cargo-leptos
# GitHub releases).  Downloading the pre-built binary is ~10× faster than compiling from
# source and avoids pulling openssl-sys (which requires perl for its configure script).
# Arch detection supports multi-arch buildx (linux/amd64 → x86_64, linux/arm64 → aarch64).
# SHA-256 pinned for reproducibility.
RUN set -eux; \
    ARCH=$(uname -m); \
    case "$ARCH" in \
      x86_64)  SHA256="895cc7ff10702b4f64b3ea5f9f0872e169c870692429650e13a3ae5550b0cdeb" ;; \
      aarch64) SHA256="d45cd862c45628c9bbca43499eb7755e10f166a752a1dbc2ef2b34b3fd6fe622" ;; \
      *) echo "Unsupported arch: $ARCH" && exit 1 ;; \
    esac; \
    TARBALL="cargo-leptos-${ARCH}-unknown-linux-musl.tar.gz"; \
    curl -fsSL "https://github.com/leptos-rs/cargo-leptos/releases/download/v0.3.6/${TARBALL}" \
         -o /tmp/cargo-leptos.tar.gz; \
    echo "${SHA256}  /tmp/cargo-leptos.tar.gz" | sha256sum -c -; \
    tar -xzf /tmp/cargo-leptos.tar.gz \
        "cargo-leptos-${ARCH}-unknown-linux-musl/cargo-leptos" \
        --strip-components=1 -C /usr/local/bin; \
    rm /tmp/cargo-leptos.tar.gz; \
    chmod +x /usr/local/bin/cargo-leptos

# Add required compilation targets.
# wasm32-unknown-unknown: the Leptos WASM frontend (always needed).
# The musl server-binary target is added dynamically below based on TARGETARCH.
RUN rustup target add wasm32-unknown-unknown

WORKDIR /app

# Copy the full workspace into the builder stage.
# .dockerignore trims target/, .git/, .planning/, data/, scratch/ from the context.
COPY . .

# Build: cargo-leptos compiles the WASM frontend first (target/site/pkg), then
# the SSR+embed-assets server binary.  bin-features = ["ssr", "embed-assets"] in
# [[workspace.metadata.leptos]] activates rust-embed so the assets are baked in.
#
# LEPTOS_BIN_TARGET_TRIPLE is set via env var ONLY — never in [[workspace.metadata.leptos]]
# in Cargo.toml, as a permanent entry there would break macOS dev builds
# (RESEARCH Open Question 2; D-04 decision).
#
# The triple is derived from the build platform (TARGETARCH is a Docker BuildKit built-in,
# automatically set to the target architecture):
#   linux/amd64  → x86_64-unknown-linux-musl
#   linux/arm64  → aarch64-unknown-linux-musl
# For a plain `docker build` (no --platform), TARGETARCH equals the host platform.
ARG TARGETARCH
RUN set -eux; \
    case "${TARGETARCH:-$(uname -m)}" in \
      amd64|x86_64)  TRIPLE=x86_64-unknown-linux-musl  ;; \
      arm64|aarch64) TRIPLE=aarch64-unknown-linux-musl ;; \
      *) echo "Unsupported TARGETARCH: ${TARGETARCH}" && exit 1 ;; \
    esac; \
    rustup target add "${TRIPLE}"; \
    LEPTOS_BIN_TARGET_TRIPLE="${TRIPLE}" cargo leptos build --release; \
    cp "/app/target/${TRIPLE}/release/ferrobucket" /app/ferrobucket-release

# ── Stage 2: Runtime (musl path — scratch) ────────────────────────────────────
# The static musl binary has no runtime dependencies; scratch is the minimal base.
# CVE surface: only the ferrobucket binary itself (no shell, no package manager).
#
# Documented glibc fallback (D-04):
#   If the musl build fails (e.g. a future C-FFI dep is introduced), replace the
#   two lines below with:
#       FROM debian:bookworm-slim
#       COPY --from=builder /app/ferrobucket-release /ferrobucket
#   Remove the LEPTOS_BIN_TARGET_TRIPLE logic, musl target add, and cp steps from
#   the builder; change CARGO_BUILD_TARGET to x86_64-unknown-linux-gnu (or omit it).
#   The resulting glibc image is ~80 MB vs <10 MB for the scratch image but is
#   otherwise functionally equivalent.
FROM scratch

# Copy the statically-linked binary from the builder.
# The binary is normalised to /app/ferrobucket-release in the builder stage so
# the runtime COPY is arch-independent (cargo places it under
# target/{triple}/release/ferrobucket when LEPTOS_BIN_TARGET_TRIPLE is set).
COPY --from=builder /app/ferrobucket-release /ferrobucket

# Run as non-root (T-05-04: Elevation of Privilege mitigation — ASVS V14).
# UID 65534 = "nobody" on most Linux systems; no /etc/passwd needed in scratch.
USER 65534

# Declare the data volume so container runtimes know /data is persistent state.
VOLUME /data

# Expose the S3 + UI port.
EXPOSE 9000

# Override the bare-binary listen default (127.0.0.1:9000) with 0.0.0.0 so the
# container port is reachable via the published mapping (D-06 / Pitfall 6).
# The bare binary's default is intentionally left as 127.0.0.1 in the source.
ENV FERROBUCKET_LISTEN=0.0.0.0:9000

# Point the data directory at the declared volume.
ENV FERROBUCKET_DATA=/data

# Do NOT bake access/secret keys here — supply at runtime via --env-file or
# Docker secrets (T-05-02: Information Disclosure mitigation — ASVS V6/V14):
#   docker run --env-file .env -p 9000:9000 -v ./data:/data ferrobucket:latest
ENTRYPOINT ["/ferrobucket", "serve"]

# ── Multi-arch build (D-05) ────────────────────────────────────────────────────
# To produce an amd64 + arm64 OCI manifest (does not require a registry push):
#
#   docker buildx build --platform linux/amd64,linux/arm64 \
#     -t ferrobucket:latest .
#
# For arm64 inside the builder stage, the ARG TARGETARCH logic sets
# LEPTOS_BIN_TARGET_TRIPLE=aarch64-unknown-linux-musl automatically when Docker
# BuildKit sets TARGETARCH=arm64.  Native arm64 runners are significantly faster
# than QEMU emulation for the Rust compile step.

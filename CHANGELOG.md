# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Click-to-zoom full-image lightbox in the object detail panel.

### Fixed
- UI uploads now store the correct `Content-Type` (from the request header, with an
  extension-based fallback), so images and text files preview correctly instead of
  being stored as `application/octet-stream`.
- The object list now refreshes after a drag-and-drop upload completes.

## [0.1.0] - 2026-06-21

### Added
- S3-compatible HTTP API via the `s3s` crate: bucket CRUD, object put/get/head/delete,
  `DeleteObjects`, `ListObjectsV2` (prefix + delimiter), multipart upload, and presigned
  GET/PUT.
- SigV4 authentication with a single static credential; `--anonymous` mode for local use.
- Own filesystem storage backend (`ferrobucket-storage`), decoupled from `s3s`.
- Built-in web UI (Leptos SSR + islands): bucket list, object browser with prefix
  navigation, upload (drag-and-drop + multipart), object detail with metadata and
  presigned URLs, settings, and light/dark themes.
- Single static release binary with embedded UI assets; multi-arch Docker image
  (`scratch`, static musl) with a ~6 MB idle memory footprint.

[Unreleased]: https://github.com/litvancom/ferrobucket/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/litvancom/ferrobucket/releases/tag/v0.1.0
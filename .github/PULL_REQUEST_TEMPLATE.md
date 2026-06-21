## Summary

What does this PR do, and why?

Closes #<!-- issue number, if applicable -->

## Changes

-

## Checklist

- [ ] `cargo fmt --all` is clean
- [ ] `cargo clippy --workspace --all-targets` has no new warnings
- [ ] `cargo test` passes (and `cargo leptos build` if the UI changed)
- [ ] Any new deviation from AWS S3 behaviour is documented in the README
- [ ] Storage stays decoupled (no `s3s` types in `ferrobucket-storage`)
- [ ] No credentials/signing logic added to the WASM/browser path

## Notes for reviewers

Anything specific you'd like feedback on.

# Security Policy

## Supported versions

ferrobucket is pre-1.0 software. Security fixes are applied to the latest `main`
and the most recent release only.

| Version | Supported |
| ------- | --------- |
| latest `main` / newest release | ✅ |
| older releases | ❌ |

## Scope & threat model

ferrobucket is designed for **local development and homelab** use, not as a
hardened, internet-facing, multi-tenant service. In particular:

- It authenticates a **single static credential** via SigV4 (or `--anonymous`,
  which disables auth entirely for local use).
- There is no multi-user IAM, ACL, or per-bucket policy.

Please keep this intended use in mind when assessing impact. Issues that require
exposing an `--anonymous` instance to an untrusted network are out of scope.

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, use GitHub's **private vulnerability reporting**:
<https://github.com/litvancom/ferrobucket/security/advisories/new>

If that is unavailable, email the maintainer at
`<SECURITY_CONTACT_EMAIL>` *(replace with a real address before publishing)*.

Please include:

- A description of the issue and its impact.
- Steps to reproduce (a minimal proof of concept if possible).
- Affected version / commit.

You can expect an initial acknowledgement within a few days. We'll work with you
on a fix and coordinate disclosure once a patch is available. Thank you for
helping keep ferrobucket and its users safe.

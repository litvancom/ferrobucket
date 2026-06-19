/// Plain byte-range type for S3 ranged GetObject (D-03, D-04).
///
/// `ByteRange` carries the client's requested byte range in its *parsed* form
/// (before resolution against the actual object length). It has **no `s3s` dependency** —
/// the conversion from `s3s::dto::Range` to `ByteRange` happens only in `s3_impl.rs`
/// (DEC-storage-decoupled rule).
///
/// ## Representation
///
/// Three constructors map to the three HTTP `Range` forms S3 supports:
/// - `full_from(first, last)` — `bytes=first-last` (inclusive both ends)
/// - `open_from(first)`      — `bytes=first-` (open-ended, read to EOF)
/// - `suffix(n)`             — `bytes=-n` (last *n* bytes of the object)
///
/// Internally the three forms are distinguished by the `kind` field so that `resolve`
/// can apply the correct clamping semantics.  Do **not** rely on `first`/`last` field
/// values when `kind == ByteRangeKind::Suffix` — only `n` is meaningful there.
#[derive(Debug, Clone, Copy)]
pub struct ByteRange {
    kind: ByteRangeKind,
}

#[derive(Debug, Clone, Copy)]
enum ByteRangeKind {
    /// `bytes=first-last` — both endpoints inclusive.
    Full { first: u64, last: u64 },
    /// `bytes=first-` — open-ended from `first` to EOF.
    Open { first: u64 },
    /// `bytes=-n` — last `n` bytes.
    Suffix { n: u64 },
}

/// A satisfiable byte window: `start` is the zero-based first byte, `length` is the
/// number of bytes to read. `start + length <= object_len` is always true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedRange {
    pub start: u64,
    pub length: u64,
}

impl ByteRange {
    /// Construct a `bytes=first-last` range (both endpoints inclusive).
    pub fn full_from(first: u64, last: u64) -> Self {
        Self { kind: ByteRangeKind::Full { first, last } }
    }

    /// Construct a `bytes=first-` open-ended range (first byte to EOF).
    pub fn open_from(first: u64) -> Self {
        Self { kind: ByteRangeKind::Open { first } }
    }

    /// Construct a `bytes=-n` suffix range (last `n` bytes of the object).
    pub fn suffix(n: u64) -> Self {
        Self { kind: ByteRangeKind::Suffix { n } }
    }

    /// Resolve this range against an object of `object_len` bytes.
    ///
    /// Returns `Some(ResolvedRange)` for satisfiable ranges, `None` for unsatisfiable
    /// ones (T-02-01; the adapter maps `None` to HTTP 416 Range Not Satisfiable).
    ///
    /// Clamping rules (S3/RFC 9110 §14.1.2):
    /// - `Full { last }`: clamp `last` to `object_len - 1`; unsatisfiable if `first >= object_len`.
    /// - `Open { first }`: unsatisfiable if `first >= object_len` (and for zero-length objects).
    /// - `Suffix { n }`: clamp `n` to `object_len` (never unsatisfiable — zero-length object
    ///   yields a zero-length window starting at byte 0, matching S3 behaviour).
    ///
    /// Security (T-02-01, T-02-02): `start + length <= object_len` is guaranteed, so callers
    /// can seek to `start` and read exactly `length` bytes without ever going past the object.
    pub fn resolve(&self, object_len: u64) -> Option<ResolvedRange> {
        match self.kind {
            ByteRangeKind::Full { first, last } => {
                if object_len == 0 || first >= object_len {
                    return None; // unsatisfiable
                }
                // Clamp last to the final byte index (T-02-02).
                let clamped_last = last.min(object_len - 1);
                // After clamping, first <= clamped_last is guaranteed because first < object_len
                // and clamped_last >= object_len - 1 >= first (first < object_len implies
                // first <= object_len - 1 = clamped_last when last >= first, which is the
                // well-formed case; if last < first the range is malformed but we still
                // have clamped_last >= first only if last >= first — handle degenerate case).
                if first > clamped_last {
                    return None; // malformed range (last < first after clamping)
                }
                Some(ResolvedRange {
                    start: first,
                    length: clamped_last - first + 1,
                })
            }
            ByteRangeKind::Open { first } => {
                if object_len == 0 || first >= object_len {
                    return None; // unsatisfiable
                }
                Some(ResolvedRange {
                    start: first,
                    length: object_len - first,
                })
            }
            ByteRangeKind::Suffix { n } => {
                // S3 clamps a suffix larger than the object to the whole object (never 416).
                let clamped_n = n.min(object_len);
                let start = object_len - clamped_n;
                Some(ResolvedRange { start, length: clamped_n })
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // D-04 case 1: bytes=0-499 over 1000 bytes → window [0, 500)
    #[test]
    fn full_range_basic() {
        let r = ByteRange::full_from(0, 499).resolve(1000);
        assert_eq!(r, Some(ResolvedRange { start: 0, length: 500 }));
    }

    // D-04 case 2: bytes=500- over 1000 bytes → window [500, 500)
    #[test]
    fn open_range_mid() {
        let r = ByteRange::open_from(500).resolve(1000);
        assert_eq!(r, Some(ResolvedRange { start: 500, length: 500 }));
    }

    // D-04 case 3 (suffix): bytes=-200 over 1000 bytes → last 200 bytes
    #[test]
    fn suffix_range_basic() {
        let r = ByteRange::suffix(200).resolve(1000);
        assert_eq!(r, Some(ResolvedRange { start: 800, length: 200 }));
    }

    // D-04 suffix clamping: bytes=-5000 over 1000 bytes → whole object
    #[test]
    fn suffix_larger_than_object_clamps() {
        let r = ByteRange::suffix(5000).resolve(1000);
        assert_eq!(r, Some(ResolvedRange { start: 0, length: 1000 }));
    }

    // D-04 full range: last clamped to object end
    #[test]
    fn full_range_last_clamped_to_object_end() {
        let r = ByteRange::full_from(500, 99999).resolve(1000);
        assert_eq!(r, Some(ResolvedRange { start: 500, length: 500 }));
    }

    // D-04 unsatisfiable: first >= object_len
    #[test]
    fn open_range_past_eof_is_unsatisfiable() {
        let r = ByteRange::open_from(99999).resolve(1000);
        assert_eq!(r, None);
    }

    // D-04: any non-suffix range on zero-length object → unsatisfiable
    #[test]
    fn full_range_zero_length_object_unsatisfiable() {
        let r = ByteRange::full_from(0, 0).resolve(0);
        assert_eq!(r, None);
    }

    #[test]
    fn open_range_zero_length_object_unsatisfiable() {
        let r = ByteRange::open_from(0).resolve(0);
        assert_eq!(r, None);
    }

    // Suffix on zero-length object → valid but zero-length window
    #[test]
    fn suffix_zero_length_object() {
        let r = ByteRange::suffix(0).resolve(0);
        assert_eq!(r, Some(ResolvedRange { start: 0, length: 0 }));
    }

    // Edge: first == object_len is unsatisfiable for Full
    #[test]
    fn full_range_first_equals_len_unsatisfiable() {
        let r = ByteRange::full_from(1000, 1999).resolve(1000);
        assert_eq!(r, None);
    }

    // Edge: first == object_len is unsatisfiable for Open
    #[test]
    fn open_range_first_equals_len_unsatisfiable() {
        let r = ByteRange::open_from(1000).resolve(1000);
        assert_eq!(r, None);
    }

    // bytes=0-0 → exactly 1 byte
    #[test]
    fn full_range_single_byte() {
        let r = ByteRange::full_from(0, 0).resolve(1000);
        assert_eq!(r, Some(ResolvedRange { start: 0, length: 1 }));
    }

    // bytes=999-999 → last byte of 1000-byte object
    #[test]
    fn full_range_last_byte() {
        let r = ByteRange::full_from(999, 999).resolve(1000);
        assert_eq!(r, Some(ResolvedRange { start: 999, length: 1 }));
    }

    // bytes=0- → whole object (open-ended from start)
    #[test]
    fn open_range_from_zero_full_object() {
        let r = ByteRange::open_from(0).resolve(1000);
        assert_eq!(r, Some(ResolvedRange { start: 0, length: 1000 }));
    }

    // Security invariant: start + length <= object_len for all satisfiable ranges
    #[test]
    fn security_invariant_no_read_past_object() {
        let cases = [
            ByteRange::full_from(0, 499),
            ByteRange::full_from(500, 99999),
            ByteRange::open_from(500),
            ByteRange::suffix(200),
            ByteRange::suffix(5000),
        ];
        let object_len = 1000u64;
        for range in &cases {
            if let Some(rr) = range.resolve(object_len) {
                assert!(
                    rr.start + rr.length <= object_len,
                    "security invariant violated for {:?}: start={} length={} object_len={}",
                    range,
                    rr.start,
                    rr.length,
                    object_len
                );
            }
        }
    }
}

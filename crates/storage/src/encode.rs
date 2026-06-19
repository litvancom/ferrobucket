use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use crate::StorageError;

/// Encode everything except the unreserved URL chars that are also filesystem-safe.
/// NON_ALPHANUMERIC encodes all non-letter/digit chars; we add back `-`, `_`, `.`, `~`.
/// Critically: `%` is encoded as `%25` and `/` as `%2F` — making encoding self-escaping
/// and injective (D-01, Pitfall 2).
pub const KEY_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .add(b'-')
    .add(b'_')
    .add(b'.')
    .add(b'~');

/// Percent-encode an S3 object key into a single flat filesystem filename.
/// Returns `StorageError::KeyTooLong` if the encoded form exceeds 255 bytes (D-02).
pub fn encode_key(key: &str) -> Result<String, StorageError> {
    let encoded = utf8_percent_encode(key, KEY_ENCODE_SET).to_string();
    if encoded.len() > 255 {
        return Err(StorageError::KeyTooLong {
            key: key.to_owned(),
        });
    }
    Ok(encoded)
}

/// Decode a percent-encoded filename back to an S3 object key.
/// Returns `StorageError::InvalidKey` if decoding fails or the decoded value
/// contains a path traversal sequence or NUL byte (D-02 traversal safety).
pub fn decode_key(encoded: &str) -> Result<String, StorageError> {
    let decoded = percent_decode_str(encoded)
        .decode_utf8()
        .map(|cow| cow.into_owned())
        .map_err(|_| StorageError::InvalidKey)?;

    // Traversal safety: reject sequences that could escape the data root.
    if decoded.contains('\0')
        || decoded == ".."
        || decoded.starts_with("../")
        || decoded.contains("/../")
    {
        return Err(StorageError::InvalidKey);
    }

    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // --- property tests (D-10) ---

    proptest! {
        #[test]
        fn encode_decode_roundtrip(key in "[ -~]{1,200}") {
            // Keys may exceed the 255-byte encoded limit for long strings of special chars.
            // If encoding succeeds, the decoded value must equal the original.
            match encode_key(&key) {
                Ok(encoded) => {
                    let decoded = decode_key(&encoded).unwrap();
                    prop_assert_eq!(key, decoded);
                }
                Err(StorageError::KeyTooLong { .. }) => {
                    // Acceptable: key encodes to > 255 bytes; skip this case.
                }
                Err(e) => {
                    panic!("unexpected error: {:?}", e);
                }
            }
        }

        #[test]
        fn no_traversal_escape(key in ".*") {
            if let Ok(encoded) = encode_key(&key) {
                prop_assert!(!encoded.contains(".."));
                prop_assert_ne!(&encoded as &str, "..");
            }
        }

        #[test]
        fn unicode_roundtrip(key in "\\PC{1,100}") {
            if let Ok(encoded) = encode_key(&key) {
                let decoded = decode_key(&encoded).unwrap();
                prop_assert_eq!(key, decoded);
            }
        }
    }

    // --- unit tests ---

    #[test]
    fn key_too_long_rejected() {
        // 86 spaces × 3 bytes each (%20) = 258 bytes — exceeds 255
        let long_key = " ".repeat(86);
        match encode_key(&long_key) {
            Err(StorageError::KeyTooLong { .. }) => {} // expected
            other => panic!("expected KeyTooLong, got {:?}", other),
        }
    }

    #[test]
    fn decode_rejects_dotdot() {
        // ".." is two dots; dots pass through our set, so encoded == ".."
        assert!(matches!(decode_key(".."), Err(StorageError::InvalidKey)));
    }

    #[test]
    fn decode_rejects_traversal_prefix() {
        // "../secret" decoded should be rejected
        let encoded = utf8_percent_encode("../secret", KEY_ENCODE_SET).to_string();
        assert!(matches!(decode_key(&encoded), Err(StorageError::InvalidKey)));
    }

    #[test]
    fn decode_rejects_embedded_traversal() {
        let encoded = utf8_percent_encode("a/../b", KEY_ENCODE_SET).to_string();
        assert!(matches!(decode_key(&encoded), Err(StorageError::InvalidKey)));
    }

    #[test]
    fn decode_rejects_nul() {
        // %00 decodes to NUL byte
        assert!(matches!(decode_key("%00"), Err(StorageError::InvalidKey)));
    }

    #[test]
    fn slash_is_encoded() {
        // "/" must be encoded (as %2F) to prevent directory traversal
        let encoded = encode_key("a/b/c").unwrap();
        assert!(!encoded.contains('/'), "slash must be percent-encoded, got: {}", encoded);
        assert_eq!(decode_key(&encoded).unwrap(), "a/b/c");
    }

    #[test]
    fn percent_in_key_roundtrips() {
        // A key containing a literal % must round-trip (Pitfall 2)
        let key = "test%20file";
        let encoded = encode_key(key).unwrap();
        assert_eq!(decode_key(&encoded).unwrap(), key);
    }
}

use percent_encoding::{utf8_percent_encode, percent_decode_str, AsciiSet, NON_ALPHANUMERIC};
use crate::StorageError;

// Stub implementations — will be replaced in GREEN phase
pub const KEY_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .add(b'-')
    .add(b'_')
    .add(b'.')
    .add(b'~');

/// Stub: always returns Err so RED tests fail on the implementation logic, not compilation.
pub fn encode_key(_key: &str) -> Result<String, StorageError> {
    Err(StorageError::InvalidKey)
}

/// Stub: always returns Err.
pub fn decode_key(_encoded: &str) -> Result<String, StorageError> {
    Err(StorageError::InvalidKey)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // --- property tests ---

    proptest! {
        #[test]
        fn encode_decode_roundtrip(key in "[ -~]{1,200}") {
            let encoded = encode_key(&key).unwrap();
            let decoded = decode_key(&encoded).unwrap();
            prop_assert_eq!(key, decoded);
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
        // A key whose every char encodes to 3 bytes (%XX) will quickly exceed 255.
        // Use 86 copies of a space (0x20 → %20, 3 bytes each = 258 bytes).
        let long_key = " ".repeat(86);
        match encode_key(&long_key) {
            Err(StorageError::KeyTooLong { .. }) => {} // expected
            other => panic!("expected KeyTooLong, got {:?}", other),
        }
    }

    #[test]
    fn decode_rejects_dotdot() {
        // An encoded string that decodes to ".." must be rejected.
        let encoded = utf8_percent_encode("..", KEY_ENCODE_SET).to_string();
        // After encoding ".." with our set, dots pass through (they're in the safe set).
        // So encoded == "..". decode_key should reject it.
        assert!(matches!(decode_key(".."), Err(StorageError::InvalidKey)));
    }

    #[test]
    fn decode_rejects_traversal_prefix() {
        // Encoded form that decodes to "../secret"
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
        // NUL byte %00 in encoded form
        assert!(matches!(decode_key("%00"), Err(StorageError::InvalidKey)));
    }
}

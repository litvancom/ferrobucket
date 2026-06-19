// IMPORTANT: must `use md5::Digest` to bring update()/finalize() into scope (Pitfall 5)
use md5::{Digest, Md5};

pub struct EtagHasher(Md5);

impl EtagHasher {
    pub fn new() -> Self {
        Self(Md5::new())
    }

    pub fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    pub fn finalize(self) -> String {
        // Lowercase hex MD5 — matches AWS ETag format for single PutObject (DEC-etag)
        let result = self.0.finalize();
        result.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etag_is_md5_hex() {
        // MD5 of empty input is the well-known constant d41d8cd98f00b204e9800998ecf8427e
        let hasher = EtagHasher::new();
        let etag = hasher.finalize();
        assert_eq!(etag, "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn etag_of_known_input() {
        // MD5("hello") = 5d41402abc4b2a76b9719d911017c592
        let mut hasher = EtagHasher::new();
        hasher.update(b"hello");
        let etag = hasher.finalize();
        assert_eq!(etag, "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn etag_is_lowercase_hex() {
        let mut hasher = EtagHasher::new();
        hasher.update(b"test");
        let etag = hasher.finalize();
        // Must be all lowercase hex characters
        assert!(etag.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
        assert_eq!(etag.len(), 32);
    }
}

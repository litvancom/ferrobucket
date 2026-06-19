// Stub — filled in Task 3.
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

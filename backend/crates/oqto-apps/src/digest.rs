use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DIGEST_DOMAIN: &[u8] = b"oqto-app-definition/v0\0";

/// Immutable Definition content digest. Equality is the only semantic callers
/// should depend on; the textual form is lowercase SHA-256 hex.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentDigest(String);

impl ContentDigest {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Digest exact validated manifest bytes and runtime bundle bytes with explicit
/// framing. Callers must provide files in bytewise relative-path order.
#[must_use]
pub fn digest_bundle<'a>(
    manifest_bytes: &[u8],
    files: impl IntoIterator<Item = (&'a Path, &'a [u8])>,
) -> ContentDigest {
    let mut digest = Sha256::new();
    digest.update(DIGEST_DOMAIN);
    update_frame(&mut digest, b"oqto-app.toml");
    update_frame(&mut digest, manifest_bytes);

    for (path, bytes) in files {
        let portable_path = path.to_string_lossy();
        update_frame(&mut digest, portable_path.as_bytes());
        update_frame(&mut digest, bytes);
    }

    ContentDigest(hex::encode(digest.finalize()))
}

fn update_frame(digest: &mut Sha256, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    digest.update(length.to_be_bytes());
    digest.update(bytes);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn digest_is_stable_and_content_sensitive() {
        let first_path = PathBuf::from("bundle/a.js");
        let second_path = PathBuf::from("bundle/index.html");
        let files = [
            (first_path.as_path(), b"a".as_slice()),
            (second_path.as_path(), b"html".as_slice()),
        ];
        let first = digest_bundle(b"manifest", files);
        let same = digest_bundle(b"manifest", files);
        let changed = digest_bundle(
            b"manifest",
            [
                (first_path.as_path(), b"b".as_slice()),
                (second_path.as_path(), b"html".as_slice()),
            ],
        );
        assert_eq!(first, same);
        assert_ne!(first, changed);
    }
}

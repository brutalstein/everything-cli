use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use ulid::Ulid;

use crate::{Result, StorageError};

/// Governance classification stored alongside ordinary durable artifacts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
    Secret,
}

impl Sensitivity {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Confidential => "confidential",
            Self::Restricted => "restricted",
            Self::Secret => "secret",
        }
    }
}

/// Project-scoped policy metadata for a content-addressed artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectMetadata {
    pub sensitivity: Sensitivity,
    pub retention_class: String,
    pub expires_at: Option<String>,
    pub pinned: bool,
}

impl ObjectMetadata {
    #[must_use]
    pub fn new(sensitivity: Sensitivity, retention_class: impl Into<String>) -> Self {
        Self {
            sensitivity,
            retention_class: retention_class.into(),
            expires_at: None,
            pinned: false,
        }
    }
}

/// Validated lowercase hexadecimal SHA-256 object identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObjectHash(String);

impl ObjectHash {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(StorageError::InvalidObjectHash(value));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(sha256_hex(bytes))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ObjectHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

pub(crate) fn object_path(objects_root: &Path, hash: &ObjectHash) -> PathBuf {
    objects_root
        .join("sha256")
        .join(&hash.as_str()[..2])
        .join(&hash.as_str()[2..])
}

pub(crate) fn persist_bytes_atomically(
    objects_root: &Path,
    tmp_root: &Path,
    bytes: &[u8],
) -> Result<ObjectHash> {
    let hash = ObjectHash::of_bytes(bytes);
    let final_path = object_path(objects_root, &hash);

    if final_path.is_file() {
        verify_file(&final_path, &hash)?;
        return Ok(hash);
    }

    let parent = final_path
        .parent()
        .expect("content-addressed path always has a parent");
    fs::create_dir_all(parent).map_err(|source| StorageError::io(parent, source))?;
    fs::create_dir_all(tmp_root).map_err(|source| StorageError::io(tmp_root, source))?;

    let mut temp_path = None;
    let mut temp_file = None;
    for _ in 0..8 {
        let candidate = tmp_root.join(format!("object-{}.tmp", Ulid::generate()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temp_path = Some(candidate);
                temp_file = Some(file);
                break;
            }
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(StorageError::io(candidate, source)),
        }
    }

    let temp_path = temp_path.ok_or_else(|| {
        StorageError::io(
            tmp_root,
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "could not allocate a unique temporary object path",
            ),
        )
    })?;
    let mut temp_file = temp_file.expect("temporary path and file are assigned together");
    temp_file
        .write_all(bytes)
        .map_err(|source| StorageError::io(&temp_path, source))?;
    temp_file
        .sync_all()
        .map_err(|source| StorageError::io(&temp_path, source))?;
    drop(temp_file);

    match fs::rename(&temp_path, &final_path) {
        Ok(()) => sync_parent_if_supported(parent)?,
        Err(source) if final_path.is_file() => {
            fs::remove_file(&temp_path).map_err(|remove_error| {
                StorageError::io(
                    &temp_path,
                    std::io::Error::new(
                        remove_error.kind(),
                        format!(
                            "rename raced with an existing object ({source}); temporary cleanup failed: {remove_error}"
                        ),
                    ),
                )
            })?;
        }
        Err(source) => {
            let _ = fs::remove_file(&temp_path);
            return Err(StorageError::io(final_path, source));
        }
    }

    verify_file(&final_path, &hash)?;
    Ok(hash)
}

pub(crate) fn read_verified(objects_root: &Path, hash: &ObjectHash) -> Result<Vec<u8>> {
    let path = object_path(objects_root, hash);
    let mut file = File::open(&path).map_err(|source| StorageError::io(&path, source))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|source| StorageError::io(&path, source))?;
    let actual = ObjectHash::of_bytes(&bytes);
    if actual != *hash {
        return Err(StorageError::ObjectCorrupt {
            expected: hash.to_string(),
            actual: actual.to_string(),
        });
    }
    Ok(bytes)
}

fn verify_file(path: &Path, expected: &ObjectHash) -> Result<()> {
    let mut file = File::open(path).map_err(|source| StorageError::io(path, source))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| StorageError::io(path, source))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = {
        let digest = hasher.finalize();
        let mut output = String::with_capacity(64);
        for byte in digest {
            use fmt::Write as _;
            write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
        }
        output
    };
    if actual != expected.as_str() {
        return Err(StorageError::ObjectCorrupt {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

#[cfg(unix)]
fn sync_parent_if_supported(parent: &Path) -> Result<()> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| StorageError::io(parent, source))
}

#[cfg(not(unix))]
fn sync_parent_if_supported(_parent: &Path) -> Result<()> {
    Ok(())
}

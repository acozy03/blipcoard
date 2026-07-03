use crate::BlipError;
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

const BLOB_DIR: &str = "blobs";
const LOCK_FILE: &str = ".lock";
const TMP_DIR: &str = "tmp";
const SHA256_PREFIX: &str = "sha256";
const DEFAULT_MAX_BLOB_BYTES: u64 = 100 * 1024 * 1024;
const SHA256_HEX_LEN: usize = 64;
const STALE_TMP_FILE_AGE: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobMetadata {
    pub blob_ref: String,
    pub content_hash: String,
    pub byte_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobStat {
    pub blob_ref: String,
    pub byte_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobGcReport {
    pub removed: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LocalBlobStore {
    root: PathBuf,
    max_blob_bytes: u64,
}

impl LocalBlobStore {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self::with_max_blob_bytes(data_dir, DEFAULT_MAX_BLOB_BYTES)
    }

    pub fn with_max_blob_bytes(data_dir: impl AsRef<Path>, max_blob_bytes: u64) -> Self {
        Self {
            root: data_dir.as_ref().join(BLOB_DIR),
            max_blob_bytes,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn recover(&self) -> Result<(), BlipError> {
        self.with_lock(|| self.recover_stale_tmp_files(STALE_TMP_FILE_AGE))
    }

    pub(crate) fn with_lock<T>(
        &self,
        operation: impl FnOnce() -> Result<T, BlipError>,
    ) -> Result<T, BlipError> {
        fs::create_dir_all(&self.root)?;
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join(LOCK_FILE))?;
        lock_file.lock_exclusive()?;
        let result = operation();
        lock_file.unlock()?;
        result
    }

    pub(crate) fn recover_stale_tmp_files(&self, min_age: Duration) -> Result<(), BlipError> {
        let tmp_dir = self.tmp_dir();
        fs::create_dir_all(&tmp_dir)?;
        let now = SystemTime::now();

        for entry in fs::read_dir(&tmp_dir)? {
            let entry = entry?;
            let path = entry.path();
            if tmp_file_is_stale(&path, now, min_age)? {
                fs::remove_file(path)?;
            }
        }

        Ok(())
    }

    pub fn write(&self, bytes: &[u8]) -> Result<BlobMetadata, BlipError> {
        self.with_lock(|| self.write_unlocked(bytes))
    }

    pub(crate) fn write_unlocked(&self, bytes: &[u8]) -> Result<BlobMetadata, BlipError> {
        let byte_size = u64::try_from(bytes.len()).map_err(|_| BlipError::BlobTooLarge {
            size_bytes: u64::MAX,
            max_bytes: self.max_blob_bytes,
        })?;

        if byte_size > self.max_blob_bytes {
            return Err(BlipError::BlobTooLarge {
                size_bytes: byte_size,
                max_bytes: self.max_blob_bytes,
            });
        }

        self.recover_stale_tmp_files(STALE_TMP_FILE_AGE)?;

        let hash = sha256_hex(bytes);
        let blob_ref = blob_ref_for_hash(&hash);
        let final_path = self.path_for_ref(&blob_ref)?;

        if final_path.exists() {
            validate_file_hash(&final_path, &hash, &blob_ref)?;
            return Ok(BlobMetadata {
                blob_ref,
                content_hash: format!("{SHA256_PREFIX}:{hash}"),
                byte_size,
            });
        }

        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = self.tmp_dir().join(format!("{}.tmp", Uuid::new_v4()));
        {
            let mut tmp_file = File::create_new(&tmp_path)?;
            tmp_file.write_all(bytes)?;
            tmp_file.sync_all()?;
        }

        match fs::hard_link(&tmp_path, &final_path) {
            Ok(()) => {
                fs::remove_file(&tmp_path)?;
            }
            Err(_) if final_path.exists() => {
                fs::remove_file(&tmp_path)?;
                validate_file_hash(&final_path, &hash, &blob_ref)?;
                return Ok(BlobMetadata {
                    blob_ref,
                    content_hash: format!("{SHA256_PREFIX}:{hash}"),
                    byte_size,
                });
            }
            Err(error) => {
                let _ = fs::remove_file(&tmp_path);
                return Err(error.into());
            }
        }

        Ok(BlobMetadata {
            blob_ref,
            content_hash: format!("{SHA256_PREFIX}:{hash}"),
            byte_size,
        })
    }

    pub fn read(&self, blob_ref: &str) -> Result<Vec<u8>, BlipError> {
        let path = self.path_for_ref(blob_ref)?;
        let bytes = fs::read(path)?;
        let expected_hash = hash_from_blob_ref(blob_ref)?;
        if sha256_hex(&bytes) != expected_hash {
            return Err(BlipError::BlobIntegrityMismatch(blob_ref.to_owned()));
        }
        Ok(bytes)
    }

    pub fn stat(&self, blob_ref: &str) -> Result<Option<BlobStat>, BlipError> {
        let path = self.path_for_ref(blob_ref)?;
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => Ok(Some(BlobStat {
                blob_ref: blob_ref.to_owned(),
                byte_size: metadata.len(),
            })),
            Ok(_) => Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn exists(&self, blob_ref: &str) -> Result<bool, BlipError> {
        Ok(self.stat(blob_ref)?.is_some())
    }

    pub fn delete(&self, blob_ref: &str) -> Result<bool, BlipError> {
        self.with_lock(|| self.delete_unlocked(blob_ref))
    }

    pub(crate) fn delete_unlocked(&self, blob_ref: &str) -> Result<bool, BlipError> {
        let path = self.path_for_ref(blob_ref)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub fn garbage_collect(
        &self,
        referenced_blob_refs: &HashSet<String>,
    ) -> Result<BlobGcReport, BlipError> {
        self.with_lock(|| self.garbage_collect_unlocked(referenced_blob_refs))
    }

    pub(crate) fn garbage_collect_unlocked(
        &self,
        referenced_blob_refs: &HashSet<String>,
    ) -> Result<BlobGcReport, BlipError> {
        self.recover_stale_tmp_files(STALE_TMP_FILE_AGE)?;

        let mut removed = Vec::new();
        for blob_ref in self.list_blob_refs()? {
            if !referenced_blob_refs.contains(&blob_ref) && self.delete_unlocked(&blob_ref)? {
                removed.push(blob_ref);
            }
        }

        removed.sort();
        Ok(BlobGcReport { removed })
    }

    fn list_blob_refs(&self) -> Result<Vec<String>, BlipError> {
        let sha_dir = self.root.join(SHA256_PREFIX);
        if !sha_dir.exists() {
            return Ok(Vec::new());
        }

        let mut blob_refs = Vec::new();
        for prefix_entry in fs::read_dir(sha_dir)? {
            let prefix_entry = prefix_entry?;
            if !prefix_entry.file_type()?.is_dir() {
                continue;
            }

            let prefix = prefix_entry.file_name().to_string_lossy().to_string();
            for blob_entry in fs::read_dir(prefix_entry.path())? {
                let blob_entry = blob_entry?;
                if !blob_entry.file_type()?.is_file() {
                    continue;
                }

                let hash = blob_entry.file_name().to_string_lossy().to_string();
                let blob_ref = format!("{SHA256_PREFIX}/{prefix}/{hash}");
                if self.path_for_ref(&blob_ref).is_ok() {
                    blob_refs.push(blob_ref);
                }
            }
        }

        Ok(blob_refs)
    }

    fn path_for_ref(&self, blob_ref: &str) -> Result<PathBuf, BlipError> {
        let Some(rest) = blob_ref.strip_prefix("sha256/") else {
            return Err(BlipError::InvalidBlobRef(blob_ref.to_owned()));
        };
        let parts = rest.split('/').collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(BlipError::InvalidBlobRef(blob_ref.to_owned()));
        }

        let prefix = parts[0];
        let hash = parts[1];
        if prefix.len() != 2
            || hash.len() != SHA256_HEX_LEN
            || !hash.starts_with(prefix)
            || !prefix
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(BlipError::InvalidBlobRef(blob_ref.to_owned()));
        }

        Ok(self.root.join(SHA256_PREFIX).join(prefix).join(hash))
    }

    fn tmp_dir(&self) -> PathBuf {
        self.root.join(TMP_DIR)
    }
}

fn blob_ref_for_hash(hash: &str) -> String {
    format!("{SHA256_PREFIX}/{}/{}", &hash[0..2], hash)
}

fn hash_from_blob_ref(blob_ref: &str) -> Result<String, BlipError> {
    let Some(hash) = blob_ref.rsplit('/').next() else {
        return Err(BlipError::InvalidBlobRef(blob_ref.to_owned()));
    };
    Ok(hash.to_owned())
}

fn validate_file_hash(path: &Path, expected_hash: &str, blob_ref: &str) -> Result<(), BlipError> {
    let bytes = fs::read(path)?;
    if sha256_hex(&bytes) == expected_hash {
        Ok(())
    } else {
        Err(BlipError::BlobIntegrityMismatch(blob_ref.to_owned()))
    }
}

fn tmp_file_is_stale(path: &Path, now: SystemTime, min_age: Duration) -> Result<bool, BlipError> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Ok(false);
    }

    let modified_at = metadata.modified()?;
    Ok(now
        .duration_since(modified_at)
        .is_ok_and(|age| age >= min_age))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(SHA256_HEX_LEN);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_stat_and_delete_blob() {
        let root = temp_path("blob-basic");
        let store = LocalBlobStore::with_max_blob_bytes(&root, 1024);

        let metadata = store.write(b"image bytes").expect("blob should write");

        assert_eq!(metadata.byte_size, 11);
        assert!(metadata.content_hash.starts_with("sha256:"));
        assert!(
            store
                .exists(&metadata.blob_ref)
                .expect("exists should work")
        );
        assert_eq!(
            store.read(&metadata.blob_ref).expect("blob should read"),
            b"image bytes"
        );
        assert_eq!(
            store.stat(&metadata.blob_ref).expect("stat should work"),
            Some(BlobStat {
                blob_ref: metadata.blob_ref.clone(),
                byte_size: 11
            })
        );
        assert!(
            store
                .delete(&metadata.blob_ref)
                .expect("delete should work")
        );
        assert!(
            !store
                .exists(&metadata.blob_ref)
                .expect("exists should work")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn identical_bytes_dedupe_to_same_blob_ref() {
        let root = temp_path("blob-dedupe");
        let store = LocalBlobStore::with_max_blob_bytes(&root, 1024);

        let first = store.write(b"same").expect("first blob should write");
        let second = store.write(b"same").expect("second blob should dedupe");

        assert_eq!(first, second);
        assert_eq!(
            store
                .garbage_collect(&HashSet::from([first.blob_ref.clone()]))
                .expect("gc should work")
                .removed,
            Vec::<String>::new()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_and_dedupe_reject_corrupt_content_addressed_file() {
        let root = temp_path("blob-corrupt");
        let store = LocalBlobStore::with_max_blob_bytes(&root, 1024);
        let metadata = store.write(b"original").expect("blob should write");
        let path = store
            .path_for_ref(&metadata.blob_ref)
            .expect("blob ref should resolve");
        fs::write(path, b"corrupt").expect("blob should corrupt");

        let read_error = store
            .read(&metadata.blob_ref)
            .expect_err("corrupt blob should fail integrity check");
        assert!(matches!(read_error, BlipError::BlobIntegrityMismatch(_)));

        let write_error = store
            .write(b"original")
            .expect_err("dedupe should reject corrupt existing blob");
        assert!(matches!(write_error, BlipError::BlobIntegrityMismatch(_)));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_recovery_removes_orphaned_temp_files() {
        let root = temp_path("blob-recover");
        let store = LocalBlobStore::with_max_blob_bytes(&root, 1024);
        let tmp_dir = store.root().join(TMP_DIR);
        fs::create_dir_all(&tmp_dir).expect("tmp dir should create");
        fs::write(tmp_dir.join("orphan.tmp"), b"partial").expect("orphan should write");

        store
            .recover_stale_tmp_files(Duration::ZERO)
            .expect("stale recovery should clean temp files");

        assert!(
            fs::read_dir(tmp_dir)
                .expect("tmp dir should read")
                .next()
                .is_none()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recover_preserves_recent_temp_files_to_avoid_racing_active_writers() {
        let root = temp_path("blob-recover-recent");
        let store = LocalBlobStore::with_max_blob_bytes(&root, 1024);
        let tmp_dir = store.root().join(TMP_DIR);
        fs::create_dir_all(&tmp_dir).expect("tmp dir should create");
        let active_tmp = tmp_dir.join("active.tmp");
        fs::write(&active_tmp, b"partial").expect("active temp should write");

        store
            .recover()
            .expect("recovery should preserve recent temp files");

        assert!(active_tmp.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn write_removes_temp_file_after_publish() {
        let root = temp_path("blob-temp-clean");
        let store = LocalBlobStore::with_max_blob_bytes(&root, 1024);

        store.write(b"published").expect("blob should write");

        assert!(
            fs::read_dir(store.root().join(TMP_DIR))
                .expect("tmp dir should read")
                .next()
                .is_none()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn write_allows_blobs_at_size_limit() {
        let root = temp_path("blob-size-limit");
        let store = LocalBlobStore::with_max_blob_bytes(&root, 4);

        let metadata = store.write(b"1234").expect("max-size blob should write");

        assert_eq!(metadata.byte_size, 4);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn write_rejects_oversized_blobs() {
        let root = temp_path("blob-too-large");
        let store = LocalBlobStore::with_max_blob_bytes(&root, 4);

        let error = store
            .write(b"12345")
            .expect_err("oversized blob should fail");

        assert!(matches!(
            error,
            BlipError::BlobTooLarge {
                size_bytes: 5,
                max_bytes: 4
            }
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_invalid_blob_refs() {
        let root = temp_path("blob-invalid-ref");
        let store = LocalBlobStore::with_max_blob_bytes(&root, 1024);
        let invalid_refs = [
            "../outside",
            "sha256/ab/ab",
            "sha256/zz/zz00000000000000000000000000000000000000000000000000000000000000",
            "sha256/AB/AB00000000000000000000000000000000000000000000000000000000000000",
            "sha256/ab/cd00000000000000000000000000000000000000000000000000000000000000",
            "sha256/ab/ab00000000000000000000000000000000000000000000000000000000000000/extra",
        ];

        for invalid_ref in invalid_refs {
            let error = store
                .read(invalid_ref)
                .expect_err("invalid read refs should fail");
            assert!(matches!(error, BlipError::InvalidBlobRef(_)));

            let error = store
                .stat(invalid_ref)
                .expect_err("invalid stat refs should fail");
            assert!(matches!(error, BlipError::InvalidBlobRef(_)));

            let error = store
                .exists(invalid_ref)
                .expect_err("invalid exists refs should fail");
            assert!(matches!(error, BlipError::InvalidBlobRef(_)));

            let error = store
                .delete(invalid_ref)
                .expect_err("invalid delete refs should fail");
            assert!(matches!(error, BlipError::InvalidBlobRef(_)));
        }

        let _ = fs::remove_dir_all(root);
    }

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("blipcoard-{label}-{}", Uuid::new_v4()))
    }
}

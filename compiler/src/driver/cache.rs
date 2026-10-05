//! Persistent reuse of checked results and build products across
//! invocations [MOD-8].
//!
//! A record is addressed by the SHA-256 of its key material: every input the
//! producing computation read, in a canonical encoding, preceded by the
//! identity of the compiler that produced it. A read trusts a record only
//! when its framing and checksum hold and its stored key material equals the
//! requested material byte for byte, so a hash collision, a truncated write,
//! a record from another compiler or a corrupted file is a miss and the
//! computation runs again. A record is published by writing a temporary file
//! and renaming it into place, so a reader sees a complete record or none.
//! Deleting the directory, or a failed write, changes only the work a later
//! invocation performs, never a verdict.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

/// Runtime hashing uses the host implementation behind a safe API; the
/// specification's constant-evaluation SHA-256 remains independent.
fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// The first bytes of every record.
const MAGIC: &[u8; 8] = b"WFCACHE1";

/// Distinguishes the temporary files of concurrent publications within one
/// process.
static PUBLICATIONS: AtomicU64 = AtomicU64::new(0);

/// The file whose modification time records the last pruning.
const PRUNE_STAMP: &str = ".pruned";
/// How often a cache directory is pruned.
const PRUNE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
/// How long another compiler's record is kept after its last write.
const FOREIGN_RECORD_AGE: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);
/// How long an interrupted publication's temporary file is kept.
const PARTIAL_FILE_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// The cache family of proof receipts [MOD-8].
const PROOF_RECEIPTS: &str = "proof-receipts";

/// One cache directory, scoped to the compiler that reads and writes it.
#[derive(Clone, Debug)]
pub struct BuildCache {
    root: PathBuf,
    compiler: [u8; 32],
    /// Proof receipts this handle found and recorded, for the build report.
    receipts_reused: Cell<u64>,
    receipts_recorded: Cell<u64>,
    /// Whether each verdict this handle already settled for exact inputs
    /// was an acceptance, by the digest of its key material: one
    /// invocation's checks consult each module's interface verdict once for
    /// every module whose closure holds it [MOD-8].
    settled: RefCell<HashMap<[u8; 32], bool>>,
}

impl BuildCache {
    /// Opens a cache directory, creating it when absent, for records of the
    /// compiler whose identity is `compiler`.
    ///
    /// # Errors
    ///
    /// Returns the I/O error that prevented creating the directory.
    pub fn open(root: &Path, compiler: [u8; 32]) -> std::io::Result<Self> {
        std::fs::create_dir_all(root)?;
        let cache = Self {
            root: root.to_path_buf(),
            compiler,
            receipts_reused: Cell::new(0),
            receipts_recorded: Cell::new(0),
            settled: RefCell::new(HashMap::new()),
        };
        cache.prune_daily();
        Ok(cache)
    }

    /// Prunes at most once a day, as the stamp file's modification time
    /// records; a failure to prune or stamp changes only the space the
    /// directory uses.
    fn prune_daily(&self) {
        let stamp = self.root.join(PRUNE_STAMP);
        let now = std::time::SystemTime::now();
        let due = std::fs::metadata(&stamp)
            .and_then(|metadata| metadata.modified())
            .map_or(true, |stamped| {
                now.duration_since(stamped)
                    .is_ok_and(|age| age >= PRUNE_INTERVAL)
            });
        if due {
            self.prune(now, FOREIGN_RECORD_AGE, PARTIAL_FILE_AGE);
            let _ = std::fs::write(&stamp, b"");
        }
    }

    /// Removes the records another compiler wrote that no write has touched
    /// for `foreign` and the temporary files of this cache's publications
    /// interrupted longer than `partial` ago. A record of this compiler is
    /// never removed, and neither is a file this cache did not write, such
    /// as an area another tool keeps; a record of another compiler can never
    /// be read by this one, and a younger one may belong to a build still in
    /// use beside it. A read does not refresh a record's time, so a record
    /// another compiler only reads is removed once it is `foreign` old and
    /// that compiler recomputes it. Another compiler may also publish a new
    /// record under a name between this pruning's check and its removal;
    /// that record is then recomputed too. Neither case can remove a record
    /// this compiler reads, or change a verdict.
    fn prune(
        &self,
        now: std::time::SystemTime,
        foreign: std::time::Duration,
        partial: std::time::Duration,
    ) {
        let Ok(families) = std::fs::read_dir(&self.root) else {
            return;
        };
        for family in families.flatten() {
            if !family.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(family.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(age) = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| now.duration_since(modified).ok())
                else {
                    continue;
                };
                let interrupted = own_partial_file(&entry.file_name().to_string_lossy());
                let remove = if interrupted {
                    age >= partial
                } else {
                    age >= foreign && self.foreign_record(&path)
                };
                if remove {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }

    /// Whether `path` holds a record of this cache's format whose key names
    /// another compiler.
    fn foreign_record(&self, path: &Path) -> bool {
        use std::io::Read;
        const COMPILER: &[u8] = b"compiler ";
        let header = MAGIC.len() + 8 + COMPILER.len() + 32;
        let mut bytes = vec![0; header];
        let read = std::fs::File::open(path).and_then(|mut file| file.read_exact(&mut bytes));
        if read.is_err() || !bytes.starts_with(MAGIC) {
            return false;
        }
        let key = &bytes[MAGIC.len() + 8..];
        key.starts_with(COMPILER) && key[COMPILER.len()..] != self.compiler
    }

    /// Whether the verdict this handle settled for exactly `material` was
    /// an acceptance, when it settled one.
    pub(crate) fn settled(&self, material: &[u8]) -> Option<bool> {
        self.settled.borrow().get(&digest(material)).copied()
    }

    /// Notes the verdict settled for exactly `material`.
    pub(crate) fn settle(&self, material: &[u8], accepted: bool) {
        self.settled.borrow_mut().insert(digest(material), accepted);
    }

    /// How many function analyses this handle's checks took from proof
    /// receipts, and how many accepted analyses they recorded [MOD-8].
    #[must_use]
    pub fn receipt_counts(&self) -> (u64, u64) {
        (self.receipts_reused.get(), self.receipts_recorded.get())
    }

    /// The payload of the complete record of `family` whose key material is
    /// exactly `material`, when one is present.
    #[must_use]
    pub fn load(&self, family: &str, material: &[u8]) -> Option<Vec<u8>> {
        let scoped = self.scoped(material);
        let bytes = std::fs::read(self.record_path(family, &scoped)).ok()?;
        decode(&bytes, &scoped)
    }

    /// Publishes the record of `family` for `material`, replacing any record
    /// already published for it.
    ///
    /// # Errors
    ///
    /// Returns the I/O error that prevented the publication; no partial
    /// record is left under the record's name.
    pub fn store(&self, family: &str, material: &[u8], payload: &[u8]) -> std::io::Result<()> {
        let scoped = self.scoped(material);
        let path = self.record_path(family, &scoped);
        let directory = self.root.join(family);
        std::fs::create_dir_all(&directory)?;
        let temporary = directory.join(format!(
            ".{}-{}.partial",
            std::process::id(),
            PUBLICATIONS.fetch_add(1, Ordering::Relaxed)
        ));
        let written = std::fs::write(&temporary, encode(&scoped, payload))
            .and_then(|()| std::fs::rename(&temporary, &path));
        if written.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        written
    }

    /// A directory of this cache for a tool that keeps its own
    /// content-addressed store, such as LLVM's ThinLTO object cache, created
    /// when absent.
    ///
    /// # Errors
    ///
    /// Returns the I/O error that prevented creating the directory.
    pub fn area(&self, name: &str) -> std::io::Result<PathBuf> {
        let area = self.root.join(name);
        std::fs::create_dir_all(&area)?;
        Ok(area)
    }

    /// The key material with the compiler identity in front of it: every
    /// record is the product of one exact compiler.
    fn scoped(&self, material: &[u8]) -> Vec<u8> {
        let mut scoped = Vec::with_capacity(material.len() + 42);
        scoped.extend_from_slice(b"compiler ");
        scoped.extend_from_slice(&self.compiler);
        scoped.push(b'\n');
        scoped.extend_from_slice(material);
        scoped
    }

    fn record_path(&self, family: &str, scoped: &[u8]) -> PathBuf {
        self.root.join(family).join(hex(&digest(scoped)))
    }
}

/// [MOD-8] a check's proof receipts live in its build cache, each record
/// keyed by the complete canonical inputs of one function's analysis.
impl crate::semantic::ProofReceipts for BuildCache {
    fn load(&self, key: &[u8]) -> Option<Vec<u8>> {
        let receipt = BuildCache::load(self, PROOF_RECEIPTS, key);
        if receipt.is_some() {
            self.receipts_reused.set(self.receipts_reused.get() + 1);
        }
        receipt
    }

    fn store(&self, key: &[u8], receipt: &[u8]) {
        // A failed publication costs only a later analysis.
        if BuildCache::store(self, PROOF_RECEIPTS, key, receipt).is_ok() {
            self.receipts_recorded.set(self.receipts_recorded.get() + 1);
        }
    }
}

/// The identity of the running compiler: the SHA-256 of its executable.
///
/// Every source judgment, lowering and runtime supply is a function of these
/// bytes, so a record another build of the compiler wrote is never read as
/// this one's.
///
/// # Errors
///
/// Returns the I/O error that prevented reading the executable.
pub fn running_compiler_identity() -> std::io::Result<[u8; 32]> {
    let executable = std::env::current_exe()?;
    Ok(digest(&std::fs::read(executable)?))
}

/// The SHA-256 of `bytes`, for key material built from inputs too large to
/// repeat in every record, such as a runtime unit set.
#[must_use]
pub fn content_digest(bytes: &[u8]) -> [u8; 32] {
    digest(bytes)
}

/// Lowercase hexadecimal of a digest, a record's file name.
/// Whether `name` is a temporary file of an interrupted [`BuildCache::store`],
/// `.<process>-<publication>.partial`.
fn own_partial_file(name: &str) -> bool {
    name.strip_prefix('.')
        .and_then(|rest| rest.strip_suffix(".partial"))
        .and_then(|rest| rest.split_once('-'))
        .is_some_and(|(process, publication)| {
            !process.is_empty()
                && !publication.is_empty()
                && process.bytes().all(|byte| byte.is_ascii_digit())
                && publication.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn hex(bytes: &[u8]) -> String {
    use core::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

fn encode(material: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(MAGIC.len() + 16 + material.len() + payload.len() + 32);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&(material.len() as u64).to_le_bytes());
    bytes.extend_from_slice(material);
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(payload);
    let checksum = digest(&bytes);
    bytes.extend_from_slice(&checksum);
    bytes
}

fn decode(bytes: &[u8], material: &[u8]) -> Option<Vec<u8>> {
    let (body, checksum) = bytes.split_at_checked(bytes.len().checked_sub(32)?)?;
    if digest(body) != checksum {
        return None;
    }
    let body = body.strip_prefix(MAGIC.as_slice())?;
    let (stored, body) = length_prefixed(body)?;
    if stored != material {
        return None;
    }
    let (payload, rest) = length_prefixed(body)?;
    rest.is_empty().then(|| payload.to_vec())
}

fn length_prefixed(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let (length, rest) = bytes.split_at_checked(8)?;
    let length = usize::try_from(u64::from_le_bytes(length.try_into().ok()?)).ok()?;
    rest.split_at_checked(length)
}

/// The fields of one record payload, each length-prefixed so no field's bytes
/// can be mistaken for another's.
#[derive(Default)]
pub(crate) struct Fields {
    bytes: Vec<u8>,
}

impl Fields {
    pub(crate) fn push(&mut self, field: &[u8]) -> &mut Self {
        self.bytes
            .extend_from_slice(&(field.len() as u64).to_le_bytes());
        self.bytes.extend_from_slice(field);
        self
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// The fields of `bytes` in order, or `None` when they are not exactly a
    /// sequence of complete fields.
    pub(crate) fn parse(mut bytes: &[u8]) -> Option<Vec<&[u8]>> {
        let mut fields = Vec::new();
        while !bytes.is_empty() {
            let (field, rest) = length_prefixed(bytes)?;
            fields.push(field);
            bytes = rest;
        }
        Some(fields)
    }
}

#[cfg(test)]
mod tests {
    use super::BuildCache;

    #[test]
    fn runtime_sha256_preserves_published_vectors_and_constant_identity() {
        assert_eq!(
            super::hex(&super::digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            super::hex(&super::digest(&million)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        // Every short padding boundary and the actual specification bytes
        // must match the independently maintained constant implementation.
        let bytes = (0..=255).collect::<Vec<u8>>();
        for length in 0..=bytes.len() {
            assert_eq!(
                super::digest(&bytes[..length]),
                crate::spec::sha256::digest(&bytes[..length])
            );
        }
        let specification = include_bytes!("../../../spec/kernel-spec.md");
        assert_eq!(
            super::digest(specification),
            crate::spec::sha256::digest(specification)
        );
    }

    fn directory(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "whitefoot-cache-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn a_record_is_read_back_only_for_its_exact_material_and_compiler() {
        let root = directory("exact");
        let cache = BuildCache::open(&root, [1; 32]).expect("open");
        cache.store("family", b"key", b"payload").expect("store");
        assert_eq!(
            cache.load("family", b"key").as_deref(),
            Some(&b"payload"[..])
        );
        assert_eq!(cache.load("family", b"other"), None);
        assert_eq!(cache.load("other", b"key"), None);
        let other_compiler = BuildCache::open(&root, [2; 32]).expect("open");
        assert_eq!(other_compiler.load("family", b"key"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pruning_removes_only_stale_foreign_records_and_interrupted_writes() {
        let root = directory("prune");
        let ours = BuildCache::open(&root, [4; 32]).expect("open");
        let theirs = BuildCache::open(&root, [5; 32]).expect("open");
        ours.store("family", b"ours-old", b"a").expect("store");
        theirs.store("family", b"theirs-old", b"b").expect("store");
        theirs.store("family", b"theirs-new", b"c").expect("store");
        let record = |cache: &BuildCache, material: &[u8]| {
            cache.record_path("family", &cache.scoped(material))
        };
        let partial = root.join("family").join(".1-1.partial");
        let other_tool = root.join("thinlto").join("llvmcache-1");
        std::fs::write(&partial, b"x").expect("partial");
        let young_partial = root.join("family").join(".1-2.partial");
        std::fs::write(&young_partial, b"x").expect("young partial");
        let other_partial = root.join("thinlto").join("llvm.partial");
        std::fs::create_dir_all(other_tool.parent().expect("area")).expect("area");
        std::fs::write(&other_tool, b"not ours").expect("other tool's file");
        std::fs::write(&other_partial, b"not ours").expect("other tool's partial file");
        let now = std::time::SystemTime::now();
        let days = |count: u64| std::time::Duration::from_secs(count * 24 * 60 * 60);
        let age = |path: &std::path::Path, by: std::time::Duration| {
            std::fs::File::options()
                .write(true)
                .open(path)
                .and_then(|file| file.set_modified(now - by))
                .expect("set modification time");
        };
        age(&record(&ours, b"ours-old"), days(30));
        age(&record(&theirs, b"theirs-old"), days(30));
        age(&partial, days(2));
        age(&young_partial, std::time::Duration::from_secs(60 * 60));
        age(&other_partial, days(30));
        age(&other_tool, days(30));
        ours.prune(now, super::FOREIGN_RECORD_AGE, super::PARTIAL_FILE_AGE);
        assert!(
            record(&ours, b"ours-old").exists(),
            "this compiler's record stays"
        );
        assert!(
            !record(&theirs, b"theirs-old").exists(),
            "a stale foreign record goes"
        );
        assert!(
            record(&theirs, b"theirs-new").exists(),
            "a recent foreign record stays"
        );
        assert!(!partial.exists(), "an interrupted write goes");
        assert!(young_partial.exists(), "a recent interrupted write stays");
        assert!(other_tool.exists(), "another tool's file stays");
        assert!(other_partial.exists(), "another tool's partial file stays");
        assert_eq!(ours.load("family", b"ours-old").as_deref(), Some(&b"a"[..]));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn opening_prunes_at_most_once_a_day() {
        let root = directory("daily");
        let theirs = BuildCache::open(&root, [6; 32]).expect("open");
        theirs.store("family", b"old", b"x").expect("store");
        let path = theirs.record_path("family", &theirs.scoped(b"old"));
        let stale =
            std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 24 * 60 * 60);
        let set = |path: &std::path::Path| {
            std::fs::File::options()
                .write(true)
                .open(path)
                .and_then(|file| file.set_modified(stale))
                .expect("set modification time");
        };
        set(&path);
        BuildCache::open(&root, [7; 32]).expect("open");
        assert!(
            path.exists(),
            "the stamp the first open wrote defers pruning"
        );
        set(&root.join(super::PRUNE_STAMP));
        BuildCache::open(&root, [7; 32]).expect("open");
        assert!(!path.exists(), "a day-old stamp lets the next open prune");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_corrupted_or_truncated_record_is_a_miss() {
        let root = directory("corrupt");
        let cache = BuildCache::open(&root, [3; 32]).expect("open");
        cache.store("family", b"key", b"payload").expect("store");
        let record = std::fs::read_dir(root.join("family"))
            .expect("family directory")
            .map(|entry| entry.expect("entry").path())
            .next()
            .expect("one record");
        let mut bytes = std::fs::read(&record).expect("read");
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xff;
        std::fs::write(&record, &bytes).expect("corrupt");
        assert_eq!(cache.load("family", b"key"), None);
        std::fs::write(&record, &bytes[..bytes.len() / 3]).expect("truncate");
        assert_eq!(cache.load("family", b"key"), None);
        cache
            .store("family", b"key", b"payload")
            .expect("republish");
        assert_eq!(
            cache.load("family", b"key").as_deref(),
            Some(&b"payload"[..])
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

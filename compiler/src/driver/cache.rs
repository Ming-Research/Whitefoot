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

use crate::spec::sha256::digest;

/// The first bytes of every record.
const MAGIC: &[u8; 8] = b"WFCACHE1";

/// Distinguishes the temporary files of concurrent publications within one
/// process.
static PUBLICATIONS: AtomicU64 = AtomicU64::new(0);

/// The cache family of proof receipts [MOD-8].
const PROOF_RECEIPTS: &str = "proof-receipts";

type MemoryRecords = HashMap<(String, Vec<u8>), Vec<u8>>;

/// One cache directory, scoped to the compiler that reads and writes it.
#[derive(Clone, Debug)]
pub struct BuildCache {
    root: PathBuf,
    compiler: [u8; 32],
    /// Proof receipts this handle found and recorded, for the build report.
    receipts_reused: Cell<u64>,
    receipts_recorded: Cell<u64>,
    bodies_checked: Cell<u64>,
    headers_checked: Cell<u64>,
    bodies_reused: Cell<u64>,
    body_modules: RefCell<std::collections::BTreeMap<String, (u64, u64)>>,
    lowerings: RefCell<std::collections::BTreeMap<String, (u64, u64)>>,
    /// Whether each verdict this handle already settled for exact inputs
    /// was an acceptance, by the digest of its key material: one
    /// invocation's checks consult each module's interface verdict once for
    /// every module whose closure holds it [MOD-8].
    settled: RefCell<HashMap<[u8; 32], bool>>,
    /// Invocation-local products when no persistent directory was requested.
    memory: Option<RefCell<MemoryRecords>>,
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
        Ok(Self {
            root: root.to_path_buf(),
            compiler,
            memory: None,
            ..Self::ephemeral()
        })
    }

    pub(super) fn ephemeral() -> Self {
        Self {
            root: PathBuf::new(),
            compiler: [0; 32],
            receipts_reused: Cell::new(0),
            receipts_recorded: Cell::new(0),
            bodies_checked: Cell::new(0),
            headers_checked: Cell::new(0),
            bodies_reused: Cell::new(0),
            body_modules: RefCell::default(),
            lowerings: RefCell::default(),
            settled: RefCell::default(),
            memory: Some(RefCell::default()),
        }
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

    /// Structural function walks performed and imported by this invocation.
    #[must_use]
    pub fn body_counts(&self) -> (u64, u64) {
        (self.bodies_checked.get(), self.bodies_reused.get())
    }

    /// Structural walks and imports grouped by declaring module; compiler
    /// prelude rows have the separate `prelude` label.
    #[must_use]
    pub fn body_module_counts(&self) -> Vec<(String, u64, u64)> {
        self.body_modules
            .borrow()
            .iter()
            .map(|(module, (checked, reused))| (module.clone(), *checked, *reused))
            .collect()
    }

    /// Function CFGs built and imported, grouped by declaring module.
    #[must_use]
    pub fn lowering_counts(&self) -> Vec<(String, u64, u64)> {
        self.lowerings
            .borrow()
            .iter()
            .map(|(module, (built, reused))| (module.clone(), *built, *reused))
            .collect()
    }

    /// Body-less callable boundaries checked by this invocation.
    #[must_use]
    pub fn header_checks(&self) -> u64 {
        self.headers_checked.get()
    }

    pub(super) fn header_checked(&self) {
        self.headers_checked.set(self.headers_checked.get() + 1);
    }

    pub(super) fn body_work(&self, module: &str, reused: bool) {
        let mut modules = self.body_modules.borrow_mut();
        let counts = modules.entry(module.to_owned()).or_default();
        if reused {
            self.bodies_reused.set(self.bodies_reused.get() + 1);
            counts.1 += 1;
        } else {
            self.bodies_checked.set(self.bodies_checked.get() + 1);
            counts.0 += 1;
        }
    }

    /// The payload of the complete record of `family` whose key material is
    /// exactly `material`, when one is present.
    #[must_use]
    pub fn load(&self, family: &str, material: &[u8]) -> Option<Vec<u8>> {
        if let Some(memory) = &self.memory {
            return memory
                .borrow()
                .get(&(family.to_owned(), material.to_vec()))
                .cloned();
        }
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
        if let Some(memory) = &self.memory {
            memory
                .borrow_mut()
                .insert((family.to_owned(), material.to_vec()), payload.to_vec());
            return Ok(());
        }
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
        if self.memory.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "an invocation-local cache has no native tool directory",
            ));
        }
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

impl crate::LoweringProducts for BuildCache {
    fn load(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.load("lowered-functions", key)
    }
    fn store(&self, key: &[u8], product: &[u8]) {
        let _ = self.store("lowered-functions", key, product);
    }
    fn lowered(&self, module: &str, reused: bool) {
        let mut counts = self.lowerings.borrow_mut();
        let counts = counts.entry(module.to_owned()).or_default();
        if reused {
            counts.1 += 1;
        } else {
            counts.0 += 1;
        }
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

use core::fmt;

/// The build-time hash implementation, tested against independent vectors,
/// also addresses the build cache's records.
pub(crate) mod sha256;

/// The SHA-256 identity of one exact kernel specification.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SpecHash([u8; 32]);

impl SpecHash {
    /// Create an identity from its exact SHA-256 bytes.
    #[must_use]
    pub const fn from_sha256(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return the exact SHA-256 bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for SpecHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for SpecHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Version label of the active kernel specification, from the generated
/// identity module.
pub const ACTIVE_KERNEL_SPEC_VERSION: &str = crate::spec_identity::SPEC_VERSION;

/// Repository-relative stable path of the active kernel specification.
pub const ACTIVE_KERNEL_SPEC_PATH: &str = "spec/kernel-spec.md";

/// Exact UTF-8 text of the active kernel specification.
pub const ACTIVE_KERNEL_SPEC_TEXT: &str = include_str!("../../spec/kernel-spec.md");

/// Exact bytes of the active kernel specification.
pub const ACTIVE_KERNEL_SPEC_BYTES: &[u8] = ACTIVE_KERNEL_SPEC_TEXT.as_bytes();

/// SHA-256 identity of the active kernel specification, decoded at compile
/// time from the generated identity module.
///
/// Decoded rather than computed here only because a constant is re-evaluated
/// in every crate that reads it, and hashing the whole specification in the
/// constant evaluator costs about twelve seconds per crate. Cargo derives
/// this value once from the same specification bytes embedded above.
pub const ACTIVE_KERNEL_SPEC_HASH: SpecHash =
    SpecHash::from_sha256(sha256_from_hex(crate::spec_identity::SPEC_SHA256_HEX));

/// Decode 64 lowercase hex digits into 32 bytes at compile time. A malformed
/// digest is a compile-time panic, never a wrong identity.
const fn sha256_from_hex(hex: &str) -> [u8; 32] {
    let digits = hex.as_bytes();
    assert!(digits.len() == 64, "a SHA-256 digest is 64 hex digits");
    let mut bytes = [0u8; 32];
    let mut index = 0;
    while index < 32 {
        bytes[index] = (hex_value(digits[2 * index]) << 4) | hex_value(digits[2 * index + 1]);
        index += 1;
    }
    bytes
}

const fn hex_value(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'a'..=b'f' => digit - b'a' + 10,
        _ => panic!("a SHA-256 digest is lowercase hex"),
    }
}

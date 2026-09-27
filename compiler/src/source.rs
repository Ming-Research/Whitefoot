use core::fmt;

/// Dense source ordinal interpreted within an accompanying source context.
///
/// This is a portable coordinate, not a validated or global identity. The same
/// value can name different files in different bundles. Code that needs a
/// bundle-bound source or span must retain the handle returned by that bundle.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceId(u32);

impl SourceId {
    /// Creates an identity from a bundle-order ordinal.
    #[must_use]
    pub const fn from_ordinal(ordinal: u32) -> Self {
        Self(ordinal)
    }

    /// Returns the bundle-order ordinal.
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.0
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// A raw byte offset within one source file.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ByteOffset(u64);

impl ByteOffset {
    /// Creates an offset from its raw byte count.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw byte count.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// A validated half-open byte range of one bundle source: the source and
/// byte offsets that [`SourceBundle::span`] checked against it. The span
/// holds no borrow of the bundle; its bytes are read through the bundle that
/// validated it ([`SourceBundle::span_bytes`]). Persisted artifact
/// coordinates will instead require the enclosing source-binding identity.
#[derive(Clone, Copy, Debug)]
pub struct SourceSpan {
    source: SourceId,
    start: ByteOffset,
    end: ByteOffset,
}

impl SourceSpan {
    /// Returns the source containing the complete span.
    #[must_use]
    pub const fn source(self) -> SourceId {
        self.source
    }

    /// Returns the inclusive starting byte offset.
    #[must_use]
    pub const fn start(self) -> ByteOffset {
        self.start
    }

    /// Returns the exclusive ending byte offset.
    #[must_use]
    pub const fn end(self) -> ByteOffset {
        self.end
    }
}

/// A portable, bundle-local source name.
///
/// Logical paths use `/` separators and ASCII components containing only
/// letters, digits, `.`, `_`, and `-`. They are never host filesystem paths.
#[derive(Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LogicalPath(String);

impl LogicalPath {
    /// Checks a logical source path without allocating or normalizing it.
    pub(crate) fn validate(value: &str) -> Result<(), LogicalPathError> {
        if value.is_empty() {
            return Err(LogicalPathError::Empty);
        }
        if value.starts_with('/') {
            return Err(LogicalPathError::Absolute);
        }
        for (index, byte) in value.bytes().enumerate() {
            let valid = byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/');
            if !valid {
                return Err(LogicalPathError::InvalidByte { index, byte });
            }
        }
        for component in value.split('/') {
            if component.is_empty() {
                return Err(LogicalPathError::EmptyComponent);
            }
            if matches!(component, "." | "..") {
                return Err(LogicalPathError::DotComponent);
            }
        }
        Ok(())
    }

    /// Validates and fallibly owns a logical source path without normalizing it.
    pub fn parse(value: &str) -> Result<Self, LogicalPathError> {
        Self::validate(value)?;
        let requested = u64::try_from(value.len()).map_err(|_| LogicalPathError::LengthOverflow)?;
        let mut owned = String::new();
        owned
            .try_reserve_exact(value.len())
            .map_err(|_| LogicalPathError::StorageUnavailable { requested })?;
        owned.push_str(value);
        Ok(Self(owned))
    }

    /// Returns the exact logical path text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LogicalPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Why a logical source path is not a portable relative path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalPathError {
    /// The path has no bytes.
    Empty,
    /// The path begins at a filesystem root.
    Absolute,
    /// A component between separators has no bytes.
    EmptyComponent,
    /// A component is `.` or `..`.
    DotComponent,
    /// A byte is outside the closed portable spelling.
    InvalidByte {
        /// Zero-based byte position.
        index: usize,
        /// Offending byte value.
        byte: u8,
    },
    /// The host path length does not fit the portable length domain.
    LengthOverflow,
    /// The allocator could not reserve the exact validated path spelling.
    StorageUnavailable {
        /// Requested path bytes.
        requested: u64,
    },
}

impl fmt::Display for LogicalPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("logical path is empty"),
            Self::Absolute => formatter.write_str("logical path is absolute"),
            Self::EmptyComponent => formatter.write_str("logical path has an empty component"),
            Self::DotComponent => formatter.write_str("logical path contains . or .."),
            Self::InvalidByte { index, byte } => {
                write!(
                    formatter,
                    "logical path byte {index} is not portable: 0x{byte:02x}"
                )
            }
            Self::LengthOverflow => formatter.write_str("logical path length exceeds u64"),
            Self::StorageUnavailable { requested } => {
                write!(
                    formatter,
                    "storage unavailable for logical path of {requested} bytes"
                )
            }
        }
    }
}

impl std::error::Error for LogicalPathError {}

/// One caller-supplied source before bundle validation.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct SourceInput<'input> {
    logical_path: &'input str,
    display_path: &'input str,
    bytes: &'input [u8],
    module: ModuleId,
    role: SourceRole,
}

impl<'input> SourceInput<'input> {
    /// The same record with other bytes, for a check that sets part of it
    /// aside [MOD-8].
    pub(crate) const fn with_bytes<'bytes>(&self, bytes: &'bytes [u8]) -> SourceInput<'bytes>
    where
        'input: 'bytes,
    {
        SourceInput {
            logical_path: self.logical_path,
            display_path: self.display_path,
            bytes,
            module: self.module,
            role: self.role,
        }
    }
}

impl fmt::Debug for SourceInput<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceInput")
            .field("logical_path", &self.logical_path)
            .field("display_path", &self.display_path)
            .field("byte_len", &self.bytes.len())
            .finish()
    }
}

impl<'input> SourceInput<'input> {
    /// Creates a borrowed input view without allocating or normalizing it.
    ///
    /// The bundle name is also the name diagnostics and the permission ledger
    /// print. A driver that read the source from a host path the closed
    /// logical domain cannot spell uses [`SourceInput::from_host_path`]
    /// instead, so the text a reader is shown stays the text they typed.
    #[must_use]
    pub const fn new(logical_path: &'input str, bytes: &'input [u8]) -> Self {
        Self {
            logical_path,
            display_path: logical_path,
            bytes,
            module: ModuleId::BUNDLE_ROOT,
            role: SourceRole::Implementation,
        }
    }

    /// Returns the module this input belongs to.
    #[must_use]
    pub const fn module(&self) -> ModuleId {
        self.module
    }

    /// Returns this input's role in its module.
    #[must_use]
    pub const fn role(&self) -> SourceRole {
        self.role
    }

    /// The program's own name for this input, the key that orders a bundle.
    pub(crate) const fn logical_path(&self) -> &'input str {
        self.logical_path
    }

    /// The name diagnostics print for this input.
    pub(crate) const fn display_path(&self) -> &'input str {
        self.display_path
    }

    /// The exact source bytes.
    pub(crate) const fn bytes(&self) -> &'input [u8] {
        self.bytes
    }

    /// Places this input in one module of a module program with its role
    /// [MOD-2]. An input made by [`SourceInput::new`] or
    /// [`SourceInput::from_host_path`] is an implementation record of a source
    /// bundle's synthetic root module until this is called.
    #[must_use]
    pub const fn in_module(self, module: ModuleId, role: SourceRole) -> Self {
        Self {
            logical_path: self.logical_path,
            display_path: self.display_path,
            bytes: self.bytes,
            module,
            role,
        }
    }

    /// Creates a borrowed input view whose diagnostics name `display_path`.
    ///
    /// Two names, because they answer two questions. `logical_path` is the
    /// program's own name for this source: portable, bundle-local, and the key
    /// that orders the bundle and detects a duplicate, so it is confined to the
    /// closed [`LogicalPath`] spelling. `display_path` is where the host read
    /// the bytes from, which is what a writer, a script, and an editor need in
    /// order to open the file the compiler is talking about, and it is
    /// unconstrained because a host path is.
    #[must_use]
    pub const fn from_host_path(
        logical_path: &'input str,
        display_path: &'input str,
        bytes: &'input [u8],
    ) -> Self {
        Self {
            logical_path,
            display_path,
            bytes,
            module: ModuleId::BUNDLE_ROOT,
            role: SourceRole::Implementation,
        }
    }
}

/// One module's dense identity within a program: its graph row position, or
/// the synthetic root module that one source bundle forms [MOD-1, MOD-9].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModuleId(u32);

impl ModuleId {
    /// The synthetic root module every source bundle forms, and the first
    /// row of a module graph.
    pub const BUNDLE_ROOT: Self = Self(0);

    /// Returns the dense row index.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Builds the identity of the module at this row index.
    #[must_use]
    pub fn from_index(index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(Self)
    }
}

/// The part a writer source plays in its module [MOD-2]: the module's one
/// interface record, `module.wfm`, or one of its implementation records.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SourceRole {
    /// The module's interface, `module.wfm`.
    Interface,
    /// A direct `.wf` implementation record, or a source bundle's record.
    Implementation,
}

/// The package a registered module belongs to [MOD-10]: the program's own,
/// which its records name `pkg`, or the standard library the toolchain
/// supplies, which every other package names `std`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Package {
    /// The program's own package.
    Program,
    /// The standard library.
    Standard,
}

impl Package {
    /// The qualifier other packages write for this package: `pkg` for the
    /// program's own, `std` for the standard library.
    #[must_use]
    pub const fn qualifier(self) -> &'static str {
        match self {
            Self::Program => "pkg",
            Self::Standard => "std",
        }
    }
}

/// One registered module: its package, its path components after the
/// package qualifier (none for the root module) and its direct dependencies
/// in written order [MOD-1, MOD-10].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleRecord {
    package: Package,
    path: Vec<String>,
    dependencies: Vec<ModuleId>,
}

impl ModuleRecord {
    /// Creates one module record of the program's own package.
    #[must_use]
    pub const fn new(path: Vec<String>, dependencies: Vec<ModuleId>) -> Self {
        Self {
            package: Package::Program,
            path,
            dependencies,
        }
    }

    /// Creates one module record of the given package.
    #[must_use]
    pub const fn in_package(
        package: Package,
        path: Vec<String>,
        dependencies: Vec<ModuleId>,
    ) -> Self {
        Self {
            package,
            path,
            dependencies,
        }
    }

    /// Returns the package the module belongs to.
    #[must_use]
    pub const fn package(&self) -> Package {
        self.package
    }

    /// Returns the path components after the package qualifier.
    #[must_use]
    pub fn path(&self) -> &[String] {
        &self.path
    }

    /// Reports whether this module is registered at `path` in `package`.
    #[must_use]
    pub fn is_at(&self, package: Package, path: &[String]) -> bool {
        self.package == package && self.path == path
    }

    /// Returns the direct dependencies in written order.
    #[must_use]
    pub fn dependencies(&self) -> &[ModuleId] {
        &self.dependencies
    }

    /// Reports whether this module lists `target` as a direct dependency.
    #[must_use]
    pub fn depends_on(&self, target: ModuleId) -> bool {
        self.dependencies.contains(&target)
    }

    /// Renders the module's qualified name as another package writes it:
    /// `pkg` or `pkg::a::b` for the program's own modules, `std::a` for the
    /// standard library's.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        let mut name = String::from(self.package.qualifier());
        for component in &self.path {
            name.push_str("::");
            name.push_str(component);
        }
        name
    }
}

/// One validated source in a bundle.
#[derive(Eq, PartialEq)]
pub struct SourceFile {
    logical_path: LogicalPath,
    display_path: String,
    bytes: Vec<u8>,
    byte_len: u64,
    prelude: Option<PreludeSource>,
    module: ModuleId,
    role: SourceRole,
}

impl fmt::Debug for SourceFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceFile")
            .field("logical_path", &self.logical_path)
            .field("display_path", &self.display_path)
            .field("byte_len", &self.byte_len)
            .finish()
    }
}

/// PRE-1 records use ordinary grammar without granting a writer a bodyless declaration form.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreludeSource {
    Opaque,
    Function,
}

impl SourceFile {
    pub(crate) const fn prelude(&self) -> Option<PreludeSource> {
        self.prelude
    }

    /// Returns the module this writer source belongs to [MOD-2].
    #[must_use]
    pub const fn module(&self) -> ModuleId {
        self.module
    }

    /// Returns this writer source's role in its module [MOD-2].
    #[must_use]
    pub const fn role(&self) -> SourceRole {
        self.role
    }

    /// Returns the portable logical source name.
    #[must_use]
    pub const fn logical_path(&self) -> &LogicalPath {
        &self.logical_path
    }

    /// Returns the name a reader is shown for this source: the host path the
    /// driver read it from, or the logical path when the caller supplied none.
    #[must_use]
    pub fn display_path(&self) -> &str {
        &self.display_path
    }

    /// Returns the exact unnormalized source bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the source length as a portable byte count.
    #[must_use]
    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }
}

/// Which implementation resource ceiling was exceeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceLimit {
    /// Bytes in the complete canonical source-binding transport.
    BindingBytes,
    /// Number of logical source files.
    Sources,
    /// Bytes in one logical path.
    LogicalPathBytes,
    /// Bytes in one host display path.
    DisplayPathBytes,
    /// Bytes in one source file.
    SourceBytes,
    /// Sum of bytes in every source file.
    TotalSourceBytes,
}

/// Explicit toolchain input ceilings, separate from Whitefoot legality.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceLimits {
    /// Maximum source count.
    pub max_sources: u32,
    /// Maximum bytes in one logical path.
    pub max_logical_path_bytes: u64,
    /// Maximum bytes in one source file.
    pub max_source_bytes: u64,
    /// Maximum total bytes across all source files.
    pub max_total_source_bytes: u64,
    /// Maximum bytes in one complete canonical source binding.
    pub max_binding_bytes: u64,
}

impl SourceLimits {
    /// Maximum counts and byte lengths accepted by the current schemas.
    ///
    /// The source count is capped at `u32::MAX`, so the final `u32` ordinal is
    /// deliberately not assigned. Production callers must use tighter limits.
    pub const REPRESENTABLE: Self = Self {
        max_sources: u32::MAX,
        max_logical_path_bytes: u64::MAX,
        max_source_bytes: u64::MAX,
        max_total_source_bytes: u64::MAX,
        max_binding_bytes: u64::MAX,
    };

    /// Maximum source count.
    #[must_use]
    pub const fn max_sources(self) -> u32 {
        self.max_sources
    }

    /// Maximum bytes in one logical path.
    #[must_use]
    pub const fn max_logical_path_bytes(self) -> u64 {
        self.max_logical_path_bytes
    }

    /// Maximum bytes in one source file.
    #[must_use]
    pub const fn max_source_bytes(self) -> u64 {
        self.max_source_bytes
    }

    /// Maximum total source bytes.
    #[must_use]
    pub const fn max_total_source_bytes(self) -> u64 {
        self.max_total_source_bytes
    }

    /// Maximum bytes in one complete canonical source binding.
    #[must_use]
    pub const fn max_binding_bytes(self) -> u64 {
        self.max_binding_bytes
    }
}

/// One closed, explicitly ordered transport collection of source files.
///
/// File order is caller-supplied source identity, not normative compilation-
/// unit or declaration order. The current lexer never crosses a file boundary.
/// Language meaning for multi-file composition remains separately gated.
///
/// A bundle is a shared handle: every syntax stage keeps one, and a clone is
/// another handle on the same sources, so no stage borrows another's input.
#[derive(Clone, Eq, PartialEq)]
pub struct SourceBundle {
    data: std::sync::Arc<BundleData>,
}

/// The sources and modules one bundle shares among its handles.
#[derive(Eq, PartialEq)]
struct BundleData {
    files: Vec<SourceFile>,
    total_bytes: u64,
    modules: Vec<ModuleRecord>,
    module_program: bool,
}

impl fmt::Debug for SourceBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceBundle")
            .field("files", &self.data.files)
            .field("total_bytes", &self.data.total_bytes)
            .field("modules", &self.data.modules)
            .field("module_program", &self.data.module_program)
            .finish()
    }
}

impl From<BundleData> for SourceBundle {
    fn from(data: BundleData) -> Self {
        Self {
            data: std::sync::Arc::new(data),
        }
    }
}

fn try_reserve_exact<T>(
    values: &mut Vec<T>,
    additional: usize,
    limit: SourceLimit,
    requested: u64,
) -> Result<(), SourceBundleError> {
    values
        .try_reserve_exact(additional)
        .map_err(|_| SourceBundleError::StorageUnavailable { limit, requested })
}

fn find_duplicate_paths(
    inputs: &[SourceInput<'_>],
) -> Result<Option<(usize, usize)>, SourceBundleError> {
    let requested =
        u64::try_from(inputs.len()).map_err(|_| SourceBundleError::ArithmeticOverflow)?;
    let mut order = Vec::new();
    try_reserve_exact(&mut order, inputs.len(), SourceLimit::Sources, requested)?;
    order.extend(0..inputs.len());
    order.sort_unstable_by(|left, right| {
        inputs[*left]
            .logical_path
            .cmp(inputs[*right].logical_path)
            .then_with(|| left.cmp(right))
    });

    let mut duplicate = None;
    for pair in order.windows(2) {
        let first = pair[0];
        let next = pair[1];
        if inputs[first].logical_path == inputs[next].logical_path
            && duplicate.is_none_or(|(_, previous)| next < previous)
        {
            duplicate = Some((first, next));
        }
    }
    Ok(duplicate)
}

impl SourceBundle {
    pub(crate) fn includes_prelude(&self) -> bool {
        self.data
            .files
            .iter()
            .filter(|file| file.prelude.is_some())
            .count()
            == crate::prelude::DECLARATIONS.len()
    }

    /// Builds a compilation source bundle including the fixed ordinary PRE-1 declarations.
    /// At least one caller-provided source record is required by PROG-2;
    /// compiler-owned prelude records do not supply that source identity.
    /// Source-only transport tooling can continue to use `with_limits`.
    ///
    /// A bundle whose records name standard library modules also carries
    /// those modules' records and forms the root module followed by the
    /// library's modules, the root module naming every one of them [PROG-2,
    /// MOD-10].
    pub fn with_prelude(
        inputs: &[SourceInput<'_>],
        limits: SourceLimits,
    ) -> Result<Self, SourceBundleError> {
        if inputs.is_empty() {
            return Err(SourceBundleError::EmptySourceSequence);
        }
        match crate::library::bundle_part(inputs) {
            Some((records, modules)) => {
                let mut bundle = Self::with_prelude_records(&records, limits)?;
                bundle.modules = modules;
                Ok(bundle.into())
            }
            None => Self::with_prelude_records(inputs, limits).map(Self::from),
        }
    }

    /// `inputs` followed by the PRE-1 records, without a library part.
    fn with_prelude_records(
        inputs: &[SourceInput<'_>],
        limits: SourceLimits,
    ) -> Result<BundleData, SourceBundleError> {
        if inputs.is_empty() {
            return Err(SourceBundleError::EmptySourceSequence);
        }
        let mut complete = inputs.to_vec();
        complete.extend(
            crate::prelude::DECLARATIONS
                .iter()
                .map(|(path, _, text)| SourceInput::new(path, text.as_bytes())),
        );
        let mut bundle = Self::from_inputs(&complete, limits, inputs.len())?;
        for (file, (_, kind, _)) in bundle.files[inputs.len()..]
            .iter_mut()
            .zip(crate::prelude::DECLARATIONS)
        {
            file.prelude = Some(*kind);
        }
        Ok(bundle)
    }

    /// Builds a bundle under explicit toolchain resource ceilings.
    pub fn with_limits(
        inputs: &[SourceInput<'_>],
        limits: SourceLimits,
    ) -> Result<Self, SourceBundleError> {
        Self::from_inputs(inputs, limits, inputs.len()).map(Self::from)
    }

    fn from_inputs(
        inputs: &[SourceInput<'_>],
        limits: SourceLimits,
        writer_count: usize,
    ) -> Result<BundleData, SourceBundleError> {
        let source_count = inputs.len();

        let source_count_u64 =
            u64::try_from(source_count).map_err(|_| SourceBundleError::ArithmeticOverflow)?;
        if source_count_u64 > u64::from(limits.max_sources) {
            return Err(SourceBundleError::LimitExceeded {
                limit: SourceLimit::Sources,
                maximum: u64::from(limits.max_sources),
                actual: source_count_u64,
            });
        }

        let mut total_bytes = 0_u64;
        for input in inputs {
            let path_len = u64::try_from(input.logical_path.len())
                .map_err(|_| SourceBundleError::ArithmeticOverflow)?;
            if path_len > limits.max_logical_path_bytes {
                return Err(SourceBundleError::LimitExceeded {
                    limit: SourceLimit::LogicalPathBytes,
                    maximum: limits.max_logical_path_bytes,
                    actual: path_len,
                });
            }
            // The display path is a host path, so its spelling is the host's;
            // only its length is this bundle's business, and it is bounded by
            // the same ceiling because it is stored the same way.
            let display_len = u64::try_from(input.display_path.len())
                .map_err(|_| SourceBundleError::ArithmeticOverflow)?;
            if display_len > limits.max_logical_path_bytes {
                return Err(SourceBundleError::LimitExceeded {
                    limit: SourceLimit::DisplayPathBytes,
                    maximum: limits.max_logical_path_bytes,
                    actual: display_len,
                });
            }
            let source_len = u64::try_from(input.bytes.len())
                .map_err(|_| SourceBundleError::ArithmeticOverflow)?;
            if source_len > limits.max_source_bytes {
                return Err(SourceBundleError::LimitExceeded {
                    limit: SourceLimit::SourceBytes,
                    maximum: limits.max_source_bytes,
                    actual: source_len,
                });
            }
            let next_total_bytes = total_bytes
                .checked_add(source_len)
                .ok_or(SourceBundleError::ArithmeticOverflow)?;
            if next_total_bytes > limits.max_total_source_bytes {
                return Err(SourceBundleError::LimitExceeded {
                    limit: SourceLimit::TotalSourceBytes,
                    maximum: limits.max_total_source_bytes,
                    actual: next_total_bytes,
                });
            }
            LogicalPath::validate(input.logical_path).map_err(SourceBundleError::LogicalPath)?;
            total_bytes = next_total_bytes;
        }

        if let Some((first, duplicate)) = find_duplicate_paths(&inputs[..writer_count])? {
            let path =
                LogicalPath::parse(inputs[duplicate].logical_path).map_err(
                    |error| match error {
                        LogicalPathError::LengthOverflow => SourceBundleError::ArithmeticOverflow,
                        LogicalPathError::StorageUnavailable { requested } => {
                            SourceBundleError::StorageUnavailable {
                                limit: SourceLimit::LogicalPathBytes,
                                requested,
                            }
                        }
                        error => SourceBundleError::LogicalPath(error),
                    },
                )?;
            let first_position =
                u32::try_from(first).map_err(|_| SourceBundleError::ArithmeticOverflow)?;
            let duplicate_position =
                u32::try_from(duplicate).map_err(|_| SourceBundleError::ArithmeticOverflow)?;
            return Err(SourceBundleError::DuplicateLogicalPath {
                path,
                first_position,
                duplicate_position,
            });
        }

        let mut files = Vec::new();
        try_reserve_exact(
            &mut files,
            source_count,
            SourceLimit::Sources,
            source_count_u64,
        )?;
        for input in inputs {
            let logical_path =
                LogicalPath::parse(input.logical_path).map_err(|error| match error {
                    LogicalPathError::LengthOverflow => SourceBundleError::ArithmeticOverflow,
                    LogicalPathError::StorageUnavailable { requested } => {
                        SourceBundleError::StorageUnavailable {
                            limit: SourceLimit::LogicalPathBytes,
                            requested,
                        }
                    }
                    error => SourceBundleError::LogicalPath(error),
                })?;
            let display_len = u64::try_from(input.display_path.len())
                .map_err(|_| SourceBundleError::ArithmeticOverflow)?;
            let mut display_path = String::new();
            display_path
                .try_reserve_exact(input.display_path.len())
                .map_err(|_| SourceBundleError::StorageUnavailable {
                    limit: SourceLimit::DisplayPathBytes,
                    requested: display_len,
                })?;
            display_path.push_str(input.display_path);
            let source_len = u64::try_from(input.bytes.len())
                .map_err(|_| SourceBundleError::ArithmeticOverflow)?;
            let mut bytes = Vec::new();
            try_reserve_exact(
                &mut bytes,
                input.bytes.len(),
                SourceLimit::SourceBytes,
                source_len,
            )?;
            bytes.extend_from_slice(input.bytes);
            files.push(SourceFile {
                logical_path,
                display_path,
                bytes,
                byte_len: source_len,
                prelude: None,
                module: input.module,
                role: input.role,
            });
        }

        Ok(BundleData {
            files,
            total_bytes,
            modules: vec![ModuleRecord::new(Vec::new(), Vec::new())],
            module_program: false,
        })
    }

    /// Builds a module program's bundle: `inputs` are the selected modules'
    /// interface and implementation records, each placed by
    /// [`SourceInput::in_module`], and `modules` the graph rows they name
    /// [MOD-1, MOD-2]. The PRE-1 records follow as for every bundle.
    pub fn with_prelude_and_modules(
        inputs: &[SourceInput<'_>],
        modules: Vec<ModuleRecord>,
        limits: SourceLimits,
    ) -> Result<Self, SourceBundleError> {
        if inputs
            .iter()
            .any(|input| input.module.index() >= modules.len())
        {
            return Err(SourceBundleError::UnknownModule);
        }
        let mut bundle = Self::with_prelude_records(inputs, limits)?;
        bundle.modules = modules;
        bundle.module_program = true;
        Ok(bundle.into())
    }

    /// Returns every registered module in graph row order; a source bundle
    /// has exactly its synthetic root module [MOD-9].
    #[must_use]
    pub fn modules(&self) -> &[ModuleRecord] {
        &self.data.modules
    }

    /// Returns one registered module.
    #[must_use]
    pub fn module(&self, id: ModuleId) -> Option<&ModuleRecord> {
        self.data.modules.get(id.index())
    }

    /// Reports whether this bundle is a module program rather than a source
    /// bundle forming one synthetic root module [MOD-9].
    #[must_use]
    pub fn is_module_program(&self) -> bool {
        self.data.module_program
    }

    /// Returns the number of ordered source files.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.files.len()
    }

    /// Returns whether the closed input contains no source files.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.files.is_empty()
    }

    /// Returns the checked sum of all source byte lengths.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.data.total_bytes
    }

    /// Returns source files in caller-supplied transport order.
    #[must_use]
    pub fn files(&self) -> &[SourceFile] {
        &self.data.files
    }

    /// Looks up a source by its bundle-order identity.
    #[must_use]
    pub fn file(&self, source: SourceId) -> Option<&SourceFile> {
        usize::try_from(source.ordinal())
            .ok()
            .and_then(|index| self.data.files.get(index))
    }

    /// Iterates in transport order with derived source identities.
    pub fn iter(&self) -> impl Iterator<Item = (SourceId, &SourceFile)> {
        (0_u32..)
            .zip(self.data.files.iter())
            .map(|(ordinal, file)| (SourceId::from_ordinal(ordinal), file))
    }

    /// Validates and creates a half-open span within one source.
    pub fn span(
        &self,
        source: SourceId,
        start: ByteOffset,
        end: ByteOffset,
    ) -> Result<SourceSpan, SpanError> {
        let file = self.file(source).ok_or(SpanError::UnknownSource(source))?;
        if start > end {
            return Err(SpanError::Reversed { start, end });
        }
        let source_len = file.byte_len();
        if end.value() > source_len {
            return Err(SpanError::OutOfBounds { end, source_len });
        }
        Ok(SourceSpan { source, start, end })
    }

    /// Returns the bytes a span of this bundle covers; `None` for a span
    /// whose range this bundle's source does not hold, which only a span
    /// another bundle validated can name.
    #[must_use]
    pub fn span_bytes(&self, span: SourceSpan) -> Option<&[u8]> {
        let start = usize::try_from(span.start.value()).ok()?;
        let end = usize::try_from(span.end.value()).ok()?;
        self.file(span.source)?.bytes().get(start..end)
    }
}

/// Why source inputs cannot form a bundle.
#[derive(Debug, Eq, PartialEq)]
pub enum SourceBundleError {
    /// A compilation invocation supplied no source record before prelude injection.
    EmptySourceSequence,
    /// One logical path is structurally invalid.
    LogicalPath(LogicalPathError),
    /// Two source positions use the same case-sensitive logical path.
    DuplicateLogicalPath {
        /// Repeated logical path.
        path: LogicalPath,
        /// First bundle position.
        first_position: u32,
        /// Repeated bundle position.
        duplicate_position: u32,
    },
    /// An explicit implementation resource ceiling was exceeded.
    LimitExceeded {
        /// Ceiling category.
        limit: SourceLimit,
        /// Configured inclusive maximum.
        maximum: u64,
        /// Attempted value.
        actual: u64,
    },
    /// The allocator could not reserve validated input storage.
    StorageUnavailable {
        /// Storage category that could not be reserved.
        limit: SourceLimit,
        /// Exact requested count or byte length.
        requested: u64,
    },
    /// A byte count cannot be represented without wrapping.
    ArithmeticOverflow,
    /// A module program placed a source in a module its graph does not register.
    UnknownModule,
}

impl fmt::Display for SourceBundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySourceSequence => {
                formatter.write_str("compilation requires at least one source record")
            }
            Self::UnknownModule => {
                formatter.write_str("a source record names a module the graph does not register")
            }
            Self::LogicalPath(error) => write!(formatter, "{error}"),
            Self::DuplicateLogicalPath {
                path,
                first_position,
                duplicate_position,
            } => write!(
                formatter,
                "duplicate logical path {path} at positions {first_position} and {duplicate_position}"
            ),
            Self::LimitExceeded {
                limit,
                maximum,
                actual,
            } => write!(
                formatter,
                "source input limit {limit:?} is {maximum}, attempted {actual}"
            ),
            Self::StorageUnavailable { limit, requested } => write!(
                formatter,
                "storage unavailable for source input {limit:?}, requested {requested}"
            ),
            Self::ArithmeticOverflow => formatter.write_str("source byte count overflow"),
        }
    }
}

impl std::error::Error for SourceBundleError {}

/// Why a requested source span is not inside one bundle source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpanError {
    /// The source ordinal is not present in the bundle.
    UnknownSource(SourceId),
    /// The inclusive start is after the exclusive end.
    Reversed {
        /// Requested start.
        start: ByteOffset,
        /// Requested end.
        end: ByteOffset,
    },
    /// The exclusive end is beyond the source byte length.
    OutOfBounds {
        /// Requested exclusive end.
        end: ByteOffset,
        /// Actual source byte length.
        source_len: u64,
    },
}

impl fmt::Display for SpanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSource(source) => write!(formatter, "unknown source {source}"),
            Self::Reversed { start, end } => write!(
                formatter,
                "source span starts at {} after ending at {}",
                start.value(),
                end.value()
            ),
            Self::OutOfBounds { end, source_len } => write!(
                formatter,
                "source span ends at {} beyond source length {source_len}",
                end.value()
            ),
        }
    }
}

impl std::error::Error for SpanError {}

#[cfg(test)]
mod tests;

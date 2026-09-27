//! Scripted linked implementations of ordinary prelude functions.
//!
//! The program is emitted once through the ordinary ABI. Tests link the
//! native library with selected syscall fixtures to force close errors,
//! short reads and writes that a real descriptor cannot reliably produce.
//! No compiler target, semantic identifier or acceptance classification
//! changes when the linked implementation is substituted.
//!
//! Every program here is retargeted to kernel spec v0.60; no test of this
//! module lost its subject rule, so none is retired. What the retarget
//! changed, and what it left standing:
//!
//! - `region { .. }` wrappers and region parameters are gone with [OWN-3],
//!   [OWN-4], [OWN-10] and [FORM-8]; their statements stay in the enclosing
//!   block and have no successor of their own.
//! - `&uniq p` and a shared `&p` collapse to the one reference kind `&p`;
//!   write permission is stated by the callee's row [EFF-1], and the rows
//!   below drop the retired `reads(p), writes(p)` pair and the multi-path
//!   entry for the same reason.
//! - `buffer_new(n, v)` and `slice_of(&b)` retire with [VIEW-1] and the
//!   v0.59 storage classes. The successors are [OP-13] `array_filled` and
//!   `box_array_filled` and [REF-4]'s range reference `&x[lo..hi]`, which the
//!   PRE-2 host functions now take directly as the parameter kind `&[u8]`.
//! - `let previous = replace outcome = e;` retires with [SET-2]. The
//!   successor is [SET-1] `set outcome = e;`, whose [WIN-3] disposition
//!   releases the old affine value.
//!
//! Four tests compile `tests/programs/io_chunked_read.wf` and
//! `tests/programs/io_write_prefix.wf`, which are still v0.59 sources owned
//! outside this module; they run against v0.60 only once those two programs
//! are ported, and nothing here was weakened to hide that.
//!
//! One known checker gap stands between the programs written here and
//! acceptance today, and the assertions state the normative verdict rather
//! than the current one, because an unimplemented feature is not a
//! source-language rejection. `box_array_filled` in the heap-record test
//! forms a runtime-capacity `Array<u8>` inside a `Box`, so the range
//! reference over `bytes.inner` depends on the `Box` content gap
//! (`check/expressions.rs` `resolve_struct_path` has no `Box` branch). No
//! program here calls a window operation, `swap` or `free_empty`, so none
//! depends on [OP-10]'s uninferred window type parameter; none names a
//! runtime-capacity `Slots` or `Ring`; and none moves out of a window slot.

use std::fmt::Write as _;

use crate::target::{TargetLayout, TargetLayoutFailure};

// The same programs run against real descriptors and scripted linked bodies.
// Both are still v0.59 sources owned by `tests/programs/`, not by this module;
// the four tests that compile them are retargeted only when those two files
// are. The patch below also reads `bytes[0_u64]` out of the first, so a port
// that moves that run into a `Box` must spell it `bytes.inner[0_u64]` here.
const CHUNKED_READ: &[u8] = include_bytes!("../../../../tests/programs/io_chunked_read.wf");
const WRITE_PREFIX: &[u8] = include_bytes!("../../../../tests/programs/io_write_prefix.wf");

fn io_error_classes() -> Vec<&'static str> {
    let declaration = crate::library::RECORDS
        .iter()
        .find_map(|(_, source)| {
            source
                .split_once("enum IoError {\n")
                .and_then(|(_, rest)| rest.split_once("\n}").map(|(body, _)| body))
        })
        .expect("the standard library declares IoError");
    declaration
        .lines()
        .filter_map(|line| line.trim().split_once('(').map(|(name, _)| name))
        .collect()
}

fn class_arms(indent: usize, named: &[(&str, &str)], default: &str) -> String {
    let pad = " ".repeat(indent);
    let inner = " ".repeat(indent + 2);
    let mut arms = String::new();
    for class in io_error_classes() {
        let body = named
            .iter()
            .find(|(spelling, _)| *spelling == class)
            .map_or(default, |(_, body)| body);
        let body: String = body
            .lines()
            .map(|line| format!("{inner}{line}\n"))
            .collect();
        arms.push_str(&format!(
            "{pad}{class}(code: c, origin: o) => {{\n{body}{pad}}}\n"
        ));
    }
    arms
}

use super::{build_linked_executable_with_library_defines, host_optimized_module, test_directory};
use std::os::unix::ffi::OsStrExt;
use std::process::Command;

/// A host error the deterministic host can be scripted to report.
///
/// Each names the C macro the generated unit reports, so the value is the
/// selected host's own, never a number this module invents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HostError {
    /// An interrupted call. A close that reports it leaves the descriptor's
    /// state unknowable, so the linked close implementation does not retry it.
    Interrupted,
    /// A readiness refusal. The deterministic adapter consumes the next
    /// scripted answer only after recording the exact readiness wait.
    WouldBlock,
    /// A device or input/output failure: the mid-stream condition a real file
    /// cannot be made to produce at a chosen call.
    DeviceFailure,
}

impl HostError {
    const fn macro_name(self) -> &'static str {
        match self {
            Self::Interrupted => "EINTR",
            Self::WouldBlock => "EAGAIN",
            Self::DeviceFailure => "EIO",
        }
    }
}

/// One scripted outcome of one host call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HostOutcome {
    /// The call succeeds naturally: a read delivers what remains of the
    /// fixture up to the requested capacity, a write accepts the whole
    /// request, a close succeeds.
    Succeed,
    /// A transfer succeeds but the host moves at most this many bytes. On a
    /// close this is simply success — a close transfers nothing.
    Accept(u64),
    /// The call fails and reports this error.
    Fail(HostError),
}

impl HostOutcome {
    /// The script entry this outcome renders as.
    ///
    /// A non-negative entry is a cap on the bytes the call may transfer, so
    /// `Succeed` is the largest representable cap — no cap at all — and a
    /// negative entry is the negated error the call reports.
    fn entry(self) -> String {
        match self {
            Self::Succeed => "LONG_MAX".to_owned(),
            Self::Accept(count) => count.to_string(),
            Self::Fail(error) => format!("-{}", error.macro_name()),
        }
    }
}

/// The one descriptor-status answer the deterministic file reports.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum HostFileStatus {
    /// The ordinary control: the provisional descriptor names a regular file.
    #[default]
    Regular,
    /// The open succeeded, but descriptor inspection identifies a directory.
    Directory,
    /// The descriptor-status facility itself fails with this target error.
    Fail(HostError),
}

/// The scripted state one deterministic-host run answers from.
///
/// A script lists the outcome of each call to one facility in call order; a
/// call past the end of a list succeeds naturally. With one fixture file that
/// is the whole configuration surface — no recorded session, no replay, and
/// no per-test host code.
#[derive(Clone, Debug, Default)]
pub(super) struct HostScript {
    file: Option<Vec<u8>>,
    file_status: HostFileStatus,
    reads: Vec<HostOutcome>,
    writes: Vec<HostOutcome>,
    closes: Vec<HostOutcome>,
}

impl HostScript {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Supplies the one fixture file a directory-relative open produces.
    ///
    /// Without it the open reports `ENOENT`. One file is what the first
    /// slice's contract tests need; a second would be a second arrangement,
    /// not a general filesystem.
    pub(super) fn file(mut self, bytes: &[u8]) -> Self {
        self.file = Some(bytes.to_vec());
        self
    }

    /// Selects the descriptor-status answer for the one fixture file.
    fn file_status(mut self, status: HostFileStatus) -> Self {
        self.file_status = status;
        self
    }

    /// Scripts the outcome of each read attempt, in call order.
    pub(super) fn reads(mut self, outcomes: &[HostOutcome]) -> Self {
        self.reads = outcomes.to_vec();
        self
    }

    /// Scripts the outcome of each write attempt, in call order.
    pub(super) fn writes(mut self, outcomes: &[HostOutcome]) -> Self {
        self.writes = outcomes.to_vec();
        self
    }

    /// Scripts the outcome of each close attempt, in call order.
    pub(super) fn closes(mut self, outcomes: &[HostOutcome]) -> Self {
        self.closes = outcomes.to_vec();
        self
    }

    /// Renders the host translation unit this script configures.
    fn unit(&self) -> String {
        let mut source = String::from(HOST_PRELUDE);
        source.push_str(&fixture_table(self.file.as_deref()));
        source.push_str(&file_status_table(self.file_status));
        source.push_str(&outcome_table("pread", &self.reads));
        source.push_str(&outcome_table("write", &self.writes));
        source.push_str(&outcome_table("close", &self.closes));
        source.push_str(HOST_FACILITIES);
        source
    }
}

/// Renders the one descriptor-status answer used by `wf_test_fstat`.
fn file_status_table(status: HostFileStatus) -> String {
    let (error, mode) = match status {
        HostFileStatus::Regular => ("0", "S_IFREG | S_IRUSR"),
        HostFileStatus::Directory => ("0", "S_IFDIR | S_IRUSR"),
        HostFileStatus::Fail(error) => (error.macro_name(), "0"),
    };
    format!(
        "static const int wf_test_file_status_error = {error};\n\
         static const unsigned int wf_test_file_status_mode = {mode};\n"
    )
}

/// Renders the one fixture file a directory-relative open produces.
fn fixture_table(file: Option<&[u8]>) -> String {
    let present = usize::from(file.is_some());
    let bytes = file.unwrap_or_default();
    let length = bytes.len();
    let mut entries = bytes
        .iter()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>();
    // A zero-length array is not valid C, so an absent or empty fixture keeps
    // one unused slot and a length of zero.
    if entries.is_empty() {
        entries.push("0".to_owned());
    }
    format!(
        "static const int wf_test_file_present = {present};\n\
         static const unsigned char wf_test_file_bytes[] = {{ {} }};\n\
         static const unsigned long wf_test_file_length = {length};\n",
        entries.join(", ")
    )
}

/// Renders one facility's scripted outcome table.
fn outcome_table(facility: &str, outcomes: &[HostOutcome]) -> String {
    let mut entries = outcomes
        .iter()
        .map(|outcome| outcome.entry())
        .collect::<Vec<_>>();
    let length = entries.len();
    if entries.is_empty() {
        entries.push("LONG_MAX".to_owned());
    }
    let mut rendered = String::new();
    writeln!(
        rendered,
        "static const long wf_test_{facility}_script[] = {{ {} }};\n\
         static const unsigned long wf_test_{facility}_scripted = {length};\n\
         static unsigned long wf_test_{facility}_calls = 0;",
        entries.join(", ")
    )
    .expect("string writes do not fail");
    rendered
}

/// The fixed head of the generated host unit.
const HOST_PRELUDE: &str = "\
/* The deterministic test host for one Whitefoot backend test. Generated by\n\
   compiler/src/backend/tests/deterministic_target.rs; never checked in. */\n\
/* The link names -std=c11, which defines __STRICT_ANSI__, and glibc then\n\
   withholds the traditional default set that S_IFREG and S_IFDIR live in.\n\
   Asking for that set back is what every compiler-owned C unit already does\n\
   for its own facilities; this unit needs it for the mode bits its scripted\n\
   fstat answers with. The macro is unknown to Darwin's headers and inert\n\
   there. */\n\
#define _DEFAULT_SOURCE 1\n\
#include <errno.h>\n\
#include <fcntl.h>\n\
#include <limits.h>\n\
#include <stdint.h>\n\
#include <stdio.h>\n\
#include <string.h>\n\
#include <sys/stat.h>\n\
#include <unistd.h>\n\
\n\
/* The descriptors the scripted opens produce. They are deliberately not real\n\
   open descriptors: nothing in the program may treat them as one, and the\n\
   two differ so a close attempt is attributable to its own resource. */\n\
#define WF_TEST_DIRECTORY 41\n\
#define WF_TEST_FILE 42\n\
\n\
";

/// The syscall fixtures linked inside the ordinary library implementation.
const HOST_FACILITIES: &str = "\
/* Every facility appends one line to the real standard error so a test can\n\
   observe host-visible facts the source cannot see — how many attempts a\n\
   release made, and against which descriptor. */\n\
static void wf_test_trace(const char *line) {\n\
    unsigned long length = 0;\n\
    while (line[length] != '\\0') {\n\
        length++;\n\
    }\n\
    ssize_t written = write(2, line, length);\n\
    (void)written;\n\
}\n\
\n\
/* One call's scripted entry. A call past the end of its script succeeds\n\
   naturally, which for a transfer is the largest possible cap. */\n\
static long wf_test_step(const long *script, unsigned long scripted,\n\
                         unsigned long *calls) {\n\
    unsigned long call = *calls;\n\
    *calls = call + 1;\n\
    return call < scripted ? script[call] : LONG_MAX;\n\
}\n\
\n\
/* Applies a non-negative script entry as a cap on a transfer length. */\n\
static unsigned long wf_test_capped(long entry, unsigned long length) {\n\
    unsigned long cap = (unsigned long)entry;\n\
    return cap < length ? cap : length;\n\
}\n\
\n\
int wf_test_open(const char *path, int flags, ...) {\n\
    char line[128];\n\
    (void)path;\n\
    (void)flags;\n\
    snprintf(line, sizeof line, \"wf_test open fd=%d\\n\", WF_TEST_DIRECTORY);\n\
    wf_test_trace(line);\n\
    return WF_TEST_DIRECTORY;\n\
}\n\
\n\
static int wf_test_openat(int directory, const char *path, int flags) {\n\
    char line[128];\n\
    (void)path;\n\
    /* The one fixture is a regular file, so a directory-only open of it\n\
       reports ENOTDIR exactly as a real host would. Without this the scripted\n\
       host would answer a directory open with a regular file, which no real\n\
       target does and which no test should be allowed to assume. */\n\
    if (wf_test_file_present && (flags & O_DIRECTORY) != 0) {\n\
        snprintf(line, sizeof line, \"wf_test openat root=%d -> notdir\\n\",\n\
                 directory);\n\
        wf_test_trace(line);\n\
        errno = ENOTDIR;\n\
        return -1;\n\
    }\n\
    if (!wf_test_file_present) {\n\
        snprintf(line, sizeof line, \"wf_test openat root=%d -> absent\\n\",\n\
                 directory);\n\
        wf_test_trace(line);\n\
        errno = ENOENT;\n\
        return -1;\n\
    }\n\
    snprintf(line, sizeof line, \"wf_test openat root=%d fd=%d\\n\", directory,\n\
             WF_TEST_FILE);\n\
    wf_test_trace(line);\n\
    return WF_TEST_FILE;\n\
}\n\
\n\
static int wf_test_fstat(int descriptor, struct stat *status) {\n\
    char line[128];\n\
    if (descriptor != WF_TEST_FILE) {\n\
        errno = EBADF;\n\
        return -1;\n\
    }\n\
    if (wf_test_file_status_error != 0) {\n\
        snprintf(line, sizeof line,\n\
                 \"wf_test fstat fd=%d outcome=error code=%d\\n\", descriptor,\n\
                 wf_test_file_status_error);\n\
        wf_test_trace(line);\n\
        errno = wf_test_file_status_error;\n\
        return -1;\n\
    }\n\
    memset(status, 0, sizeof *status);\n\
    status->st_mode = wf_test_file_status_mode;\n\
    snprintf(line, sizeof line, \"wf_test fstat fd=%d outcome=ok\\n\", descriptor);\n\
    wf_test_trace(line);\n\
    return 0;\n\
}\n\
\n\
static ssize_t wf_test_pread(int descriptor, void *destination, size_t capacity,\n\
                             int64_t offset) {\n\
    char line[192];\n\
    long entry;\n\
    for (;;) {\n\
        entry = wf_test_step(wf_test_pread_script, wf_test_pread_scripted,\n\
                             &wf_test_pread_calls);\n\
        if (entry == -EINTR) {\n\
            snprintf(line, sizeof line,\n\
                     \"wf_test pread fd=%d progress=eintr\\n\", descriptor);\n\
            wf_test_trace(line);\n\
            continue;\n\
        }\n\
        if (entry == -EAGAIN) {\n\
            snprintf(line, sizeof line,\n\
                     \"wf_test pread fd=%d wait=readable\\n\", descriptor);\n\
            wf_test_trace(line);\n\
            continue;\n\
        }\n\
        break;\n\
    }\n\
    if (entry < 0) {\n\
        snprintf(line, sizeof line,\n\
                 \"wf_test pread fd=%d offset=%lld capacity=%lu -> error\\n\",\n\
                 descriptor, (long long)offset, (unsigned long)capacity);\n\
        wf_test_trace(line);\n\
        errno = (int)(-entry);\n\
        return -1;\n\
    }\n\
    unsigned long position = (unsigned long)offset;\n\
    unsigned long remaining = position < wf_test_file_length\n\
        ? wf_test_file_length - position : 0;\n\
    unsigned long delivered = capacity < remaining ? capacity : remaining;\n\
    delivered = wf_test_capped(entry, delivered);\n\
    memcpy(destination, wf_test_file_bytes + position, delivered);\n\
    snprintf(line, sizeof line,\n\
             \"wf_test pread fd=%d offset=%lld capacity=%lu delivered=%lu\\n\",\n\
             descriptor, (long long)offset, (unsigned long)capacity, delivered);\n\
    wf_test_trace(line);\n\
    return (ssize_t)delivered;\n\
}\n\
\n\
/* One unpositioned stream read, answered from the same fixture and\n\
   the same read script the positioned route uses. The position it reads at is\n\
   this host's own, exactly as a real descriptor's is: one cursor, advanced by\n\
   what each call delivered. */\n\
static unsigned long wf_test_stream_position = 0;\n\
\n\
static ssize_t wf_test_read(int descriptor, void *destination, size_t capacity) {\n\
    char line[192];\n\
    long entry;\n\
    for (;;) {\n\
        entry = wf_test_step(wf_test_pread_script, wf_test_pread_scripted,\n\
                             &wf_test_pread_calls);\n\
        if (entry == -EINTR) {\n\
            snprintf(line, sizeof line,\n\
                     \"wf_test read fd=%d progress=eintr\\n\", descriptor);\n\
            wf_test_trace(line);\n\
            continue;\n\
        }\n\
        if (entry == -EAGAIN) {\n\
            snprintf(line, sizeof line,\n\
                     \"wf_test read fd=%d wait=readable\\n\", descriptor);\n\
            wf_test_trace(line);\n\
            continue;\n\
        }\n\
        break;\n\
    }\n\
    if (entry < 0) {\n\
        snprintf(line, sizeof line,\n\
                 \"wf_test read fd=%d capacity=%lu -> error\\n\", descriptor,\n\
                 (unsigned long)capacity);\n\
        wf_test_trace(line);\n\
        errno = (int)(-entry);\n\
        return -1;\n\
    }\n\
    unsigned long position = wf_test_stream_position;\n\
    unsigned long remaining = position < wf_test_file_length\n\
        ? wf_test_file_length - position : 0;\n\
    unsigned long delivered = capacity < remaining ? capacity : remaining;\n\
    delivered = wf_test_capped(entry, delivered);\n\
    memcpy(destination, wf_test_file_bytes + position, delivered);\n\
    wf_test_stream_position = position + delivered;\n\
    snprintf(line, sizeof line,\n\
             \"wf_test read fd=%d position=%lu capacity=%lu delivered=%lu\\n\",\n\
             descriptor, position, (unsigned long)capacity, delivered);\n\
    wf_test_trace(line);\n\
    return (ssize_t)delivered;\n\
}\n\
\n\
static ssize_t wf_test_write(int descriptor, const void *source, size_t count) {\n\
    char line[192];\n\
    long entry;\n\
    for (;;) {\n\
        entry = wf_test_step(wf_test_write_script, wf_test_write_scripted,\n\
                             &wf_test_write_calls);\n\
        if (entry == -EINTR) {\n\
            snprintf(line, sizeof line,\n\
                     \"wf_test write fd=%d progress=eintr\\n\", descriptor);\n\
            wf_test_trace(line);\n\
            continue;\n\
        }\n\
        if (entry == -EAGAIN) {\n\
            snprintf(line, sizeof line,\n\
                     \"wf_test write fd=%d wait=writable\\n\", descriptor);\n\
            wf_test_trace(line);\n\
            continue;\n\
        }\n\
        break;\n\
    }\n\
    if (entry < 0) {\n\
        snprintf(line, sizeof line, \"wf_test write fd=%d count=%lu -> error\\n\",\n\
                 descriptor, (unsigned long)count);\n\
        wf_test_trace(line);\n\
        errno = (int)(-entry);\n\
        return -1;\n\
    }\n\
    unsigned long accepted = wf_test_capped(entry, (unsigned long)count);\n\
    /* The accepted bytes are echoed so a test can see exactly what the sink\n\
       received, which is what a real destination would show. */\n\
    snprintf(line, sizeof line, \"wf_test write fd=%d count=%lu accepted=%lu \"\n\
             \"bytes=%.*s\\n\", descriptor, (unsigned long)count, accepted,\n\
             (int)accepted, (const char *)source);\n\
    wf_test_trace(line);\n\
    return (ssize_t)accepted;\n\
}\n\
\n\
static int wf_test_close(int descriptor) {\n\
    char line[128];\n\
    long outcome = wf_test_step(wf_test_close_script, wf_test_close_scripted,\n\
                                &wf_test_close_calls);\n\
    snprintf(line, sizeof line, \"wf_test close fd=%d outcome=%s\\n\", descriptor,\n\
             outcome < 0 ? \"error\" : \"ok\");\n\
    wf_test_trace(line);\n\
    if (outcome < 0) {\n\
        errno = (int)(-outcome);\n\
        return -1;\n\
    }\n\
    return 0;\n\
}\n\
\n\
/* The one lowering, answered by this target's own column.\n\
 *\n\
 * Each linked implementation reserves a record in its own frame, submits the\n\
 * operation into it, and joins it where the outcome is needed\n\
 * (research/investigations/io-model/PARK-ON-MISS.md section 8). This host\n\
 * answers the pair the way the design's second shape admits: the engine\n\
 * executes the operation inside the submitting call and publishes the\n\
 * completion into the record, so a scripted condition is still observed\n\
 * through exactly the emitted lowering and every host attempt above is still\n\
 * traced once, in the same words. */\n\
typedef struct wf_test_completion {\n\
    int64_t value;\n\
    int error_code;\n\
    unsigned open_outcome;\n\
} wf_test_completion;\n\
\n\
/* The emitter reserves WF_COMPLETION_RECORD_BYTES at WF_COMPLETION_RECORD_ALIGN\n\
   for every operation. What a target keeps there is its own business; this one\n\
   keeps the published outcome, and says so it cannot outgrow the block. */\n\
_Static_assert(sizeof(wf_test_completion) <= 128, \"the record block holds it\");\n\
_Static_assert(_Alignof(wf_test_completion) <= 8, \"the record block aligns it\");\n\
\n\
/* The five open outcomes the emitted mapper switches on, in the order the\n\
   completion contract states them. */\n\
#define WF_TEST_OPEN_SUCCEEDED 0u\n\
#define WF_TEST_OPEN_FAILED 1u\n\
#define WF_TEST_OPEN_STATUS_FAILED 2u\n\
#define WF_TEST_OPEN_IS_DIRECTORY 3u\n\
#define WF_TEST_OPEN_OTHER_KIND 4u\n\
#define WF_TEST_EXPECT_ANY 0u\n\
#define WF_TEST_EXPECT_REGULAR 1u\n\
#define WF_TEST_EXPECT_DIRECTORY 2u\n\
\n\
static void wf_test_publish(void *record, int64_t value, int error_code,\n\
                            unsigned open_outcome) {\n\
    wf_test_completion completion;\n\
    completion.value = value;\n\
    completion.error_code = error_code;\n\
    completion.open_outcome = open_outcome;\n\
    memcpy(record, &completion, sizeof completion);\n\
}\n\
\n\
/* The descriptor-kind check and the close of a provisional descriptor that\n\
   fails it belong to whoever answers the submit, so they are here rather than\n\
   in the wrapper. */\n\
void wf_test_open_at_submit(int directory, const char *path, int flags,\n\
                            unsigned mode, unsigned has_mode,\n\
                            unsigned expected_kind, void *record) {\n\
    struct stat status;\n\
    int descriptor;\n\
    (void)mode;\n\
    (void)has_mode;\n\
    descriptor = wf_test_openat(directory, path, flags);\n\
    if (descriptor < 0) {\n\
        wf_test_publish(record, -1, errno, WF_TEST_OPEN_FAILED);\n\
        return;\n\
    }\n\
    if (expected_kind == WF_TEST_EXPECT_ANY) {\n\
        wf_test_publish(record, descriptor, 0, WF_TEST_OPEN_SUCCEEDED);\n\
        return;\n\
    }\n\
    if (wf_test_fstat(descriptor, &status) != 0) {\n\
        int inspection = errno;\n\
        (void)wf_test_close(descriptor);\n\
        wf_test_publish(record, -1, inspection, WF_TEST_OPEN_STATUS_FAILED);\n\
        return;\n\
    }\n\
    if ((expected_kind == WF_TEST_EXPECT_REGULAR && S_ISREG(status.st_mode))\n\
        || (expected_kind == WF_TEST_EXPECT_DIRECTORY\n\
            && S_ISDIR(status.st_mode))) {\n\
        wf_test_publish(record, descriptor, 0, WF_TEST_OPEN_SUCCEEDED);\n\
        return;\n\
    }\n\
    (void)wf_test_close(descriptor);\n\
    wf_test_publish(record, -1, 0,\n\
                    S_ISDIR(status.st_mode) ? WF_TEST_OPEN_IS_DIRECTORY\n\
                                            : WF_TEST_OPEN_OTHER_KIND);\n\
}\n\
\n\
/* An empty transfer has no external action at all, so it reaches no scripted\n\
   facility and consumes no script entry. */\n\
void wf_test_pread_submit(int descriptor, void *destination, uint64_t count,\n\
                          uint64_t file_offset, void *record) {\n\
    ssize_t moved;\n\
    if (count == 0) {\n\
        wf_test_publish(record, 0, 0, WF_TEST_OPEN_SUCCEEDED);\n\
        return;\n\
    }\n\
    if (file_offset > (uint64_t)INT64_MAX) {\n\
        wf_test_publish(record, -1, EINVAL, WF_TEST_OPEN_SUCCEEDED);\n\
        return;\n\
    }\n\
    moved = wf_test_pread(descriptor, destination, (size_t)count,\n\
                          (int64_t)file_offset);\n\
    wf_test_publish(record, moved < 0 ? -1 : (int64_t)moved,\n\
                    moved < 0 ? errno : 0, WF_TEST_OPEN_SUCCEEDED);\n\
}\n\
\n\
void wf_test_read_submit(int descriptor, void *destination, uint64_t count,\n\
                         void *record) {\n\
    ssize_t moved;\n\
    if (count == 0) {\n\
        wf_test_publish(record, 0, 0, WF_TEST_OPEN_SUCCEEDED);\n\
        return;\n\
    }\n\
    moved = wf_test_read(descriptor, destination, (size_t)count);\n\
    wf_test_publish(record, moved < 0 ? -1 : (int64_t)moved,\n\
                    moved < 0 ? errno : 0, WF_TEST_OPEN_SUCCEEDED);\n\
}\n\
\n\
void wf_test_write_submit(int descriptor, const void *source, uint64_t count,\n\
                          void *record) {\n\
    ssize_t accepted;\n\
    if (count == 0) {\n\
        wf_test_publish(record, 0, 0, WF_TEST_OPEN_SUCCEEDED);\n\
        return;\n\
    }\n\
    accepted = wf_test_write(descriptor, source, (size_t)count);\n\
    wf_test_publish(record, accepted < 0 ? -1 : (int64_t)accepted,\n\
                    accepted < 0 ? errno : 0, WF_TEST_OPEN_SUCCEEDED);\n\
}\n\
\n\
void wf_test_close_submit(int descriptor, void *record) {\n\
    int outcome = wf_test_close(descriptor);\n\
    wf_test_publish(record, outcome, outcome != 0 ? errno : 0,\n\
                    WF_TEST_OPEN_SUCCEEDED);\n\
}\n\
\n\
void wf_test_file_join(const void *record, int64_t *value, int *error_code) {\n\
    wf_test_completion completion;\n\
    memcpy(&completion, record, sizeof completion);\n\
    *value = completion.value;\n\
    *error_code = completion.error_code;\n\
}\n\
\n\
void wf_test_file_open_join(const void *record, int64_t *value,\n\
                            int *error_code, unsigned *open_outcome) {\n\
    wf_test_completion completion;\n\
    memcpy(&completion, record, sizeof completion);\n\
    *value = completion.value;\n\
    *error_code = completion.error_code;\n\
    *open_outcome = completion.open_outcome;\n\
}\n";

/// One run of one program against the deterministic test target.
pub(super) struct DeterministicRun {
    /// The compiled program's own result.
    pub(super) output: std::process::Output,
}

impl DeterministicRun {
    /// The host trace the scripted facilities produced.
    pub(super) fn trace(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    /// How many times one facility was reached.
    pub(super) fn attempts(&self, facility: &str) -> usize {
        let prefix = format!("wf_test {facility} ");
        self.trace()
            .lines()
            .filter(|line| line.starts_with(&prefix))
            .count()
    }
}

/// Emits one source against the deterministic test target.
pub(super) fn emit_for_deterministic_target(source: &[u8]) -> String {
    super::compile(source)
}

/// Compiles one source against the deterministic test target, links it with
/// the unit the script configures, and runs it.
pub(super) fn run_on_deterministic_host(
    source: &[u8],
    script: &HostScript,
    arguments: &[&[u8]],
) -> DeterministicRun {
    let llvm = emit_for_deterministic_target(source);
    run_emitted_on_deterministic_host(&llvm, script, arguments)
}

/// Runs an already-emitted deterministic-target module, so a caller holding a
/// shared emission does not pay a second front-end pass for the same source.
pub(super) fn run_emitted_on_deterministic_host(
    llvm: &str,
    script: &HostScript,
    arguments: &[&[u8]],
) -> DeterministicRun {
    let directory = test_directory();
    let defines = [
        "wf__completion_file_open_at_submit=wf_test_open_at_submit",
        "wf__completion_file_pread_submit=wf_test_pread_submit",
        "wf__completion_file_read_submit=wf_test_read_submit",
        "wf__completion_file_write_submit=wf_test_write_submit",
        "wf__completion_file_close_submit=wf_test_close_submit",
        "wf__completion_file_join=wf_test_file_join",
        "wf__completion_file_open_join=wf_test_file_open_join",
    ]
    .map(str::to_owned);
    let executable = build_linked_executable_with_library_defines(
        llvm,
        Some(&script.unit()),
        &[],
        &defines,
        &directory,
    );
    let output = Command::new(&executable)
        .args(
            arguments
                .iter()
                .map(|bytes| std::ffi::OsStr::from_bytes(bytes)),
        )
        .output()
        .expect("run the ordinary linked library with scripted syscalls");
    std::fs::remove_dir_all(directory).expect("remove scripted library fixture");
    DeterministicRun { output }
}

/// An ordinary entry that explicitly closes its initial working directory.
const RELEASES_ONE_DIRECTORY: &[u8] = br#"fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  let std::process::Inputs(args: args, cwd: cwd, stdout: out, stderr: err, handles: factory, stdin: input) = move inputs;
  let closed = std::fs::close_directory(factory: &factory, directory: move cwd);
  return std::process::exit_status(code: 0_u8);
}
"#;

/// A command that reads its own invocation vector and reaches no host object
/// at all, so every row it uses is one both target columns share.
const READS_ITS_ARGUMENTS: &[u8] =
    br#"fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  let std::process::Inputs(args: args, cwd: unused_cwd, stdout: unused_stdout, stderr: unused_stderr, handles: entry_factory, stdin: unused_stdin) = move inputs;
  std::fs::close_directory(factory: &entry_factory, directory: move unused_cwd);
  let total = std::text::args_count(args: &args);
  let narrowed = cvt.checked::<u64, u8>(total);
  match narrowed {
    Ok(value: code) => {
      return std::process::exit_status(code: code);
    }
    Err(error: overflowed) => {
      return std::process::exit_status(code: 200_u8);
    }
  }
}
"#;

/// Publishes three bytes to standard output and returns the accepted count,
/// while also binding the initial working directory so exactly one resource
/// in the program releases with a close.
const WRITES_THEN_RELEASES_BOTH: &[u8] =
    br#"fn exercise(cwd: &std::fs::DirectoryRead, out: &std::io::OutputStream, entry_factory: &std::io::HandleFactory) -> status: std::process::ExitStatus writes(out), writes(entry_factory) {
  let bytes = array_filled::<u8, 3>(value: 65_u8);
  set bytes[1_u64] = 66_u8;
  set bytes[2_u64] = 67_u8;
  let payload = &bytes[0_u64..3_u64];
  match std::io::write_once(factory: entry_factory, output: out, source: payload, start: 0_u64, end: 3_u64) {
    Ok(value: written) => {
      let narrowed = cvt.checked::<u64, u8>(written);
      match narrowed {
        Ok(value: code) => {
          return std::process::exit_status(code: code);
        }
        Err(error: overflowed) => {
          return std::process::exit_status(code: 200_u8);
        }
      }
    }
    Err(error: problem) => {
      return std::process::exit_status(code: 211_u8);
    }
  }
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  doc "PRE-2 ordinary std::process::Inputs are destructured once; the borrowed operation chain returns before the initial directory is explicitly closed on every exit.";
  let std::process::Inputs(args: unused_args, cwd: cwd, stdout: out, stderr: unused_stderr, handles: entry_factory, stdin: unused_stdin) = move inputs;
  let outcome = exercise(cwd: &cwd, out: &out, entry_factory: &entry_factory);
  std::fs::close_directory(factory: &entry_factory, directory: move cwd);
  return move outcome;
}
"#;

/// Opens the deterministic fixture through `open_file` and makes a selected
/// error class and its detail visible as the command status. Descriptor
/// inspection and provisional cleanup are therefore on the same emitted path
/// under test.
fn opens_one_file(named: &[(&str, &str)], default: &str) -> String {
    let arms = class_arms(8, named, default);
    format!(
        r#"fn exercise(factory: &std::io::HandleFactory, cwd: &std::fs::DirectoryRead) -> status: std::process::ExitStatus reads(cwd), writes(factory) {{
  let name = array_filled::<u8, 1>(value: 65_u8);
  let component = &name[0_u64..1_u64];
  match std::fs::open_file(factory: factory, root: cwd, name: component, start: 0_u64, end: 1_u64) {{
    Ok(value: file) => {{
      std::fs::close_read(factory: factory, file: move file);
      return std::process::exit_status(code: 24_u8);
    }}
    Err(error: problem) => {{
      match problem {{
{arms}      }}
    }}
  }}
}}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {{
  let std::process::Inputs(args: args, cwd: cwd, stdout: out, stderr: err, handles: factory, stdin: input) = move inputs;
  let outcome = std::process::exit_status(code: 0_u8);
  set outcome = exercise(factory: &factory, cwd: &cwd);
  std::fs::close_directory(factory: &factory, directory: move cwd);
  return move outcome;
}}
"#
    )
}

#[test]
fn every_explicit_target_layout_pins_its_exact_abi_bytes() {
    let cases = [
        (
            "aarch64-apple-darwin",
            "e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32",
            "__chkstk_darwin",
        ),
        (
            "x86_64-apple-darwin",
            "e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
            "__chkstk_darwin",
        ),
        (
            "aarch64-unknown-linux-gnu",
            "e-m:e-p270:32:32-p271:32:32-p272:64:64-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128",
            "inline-asm",
        ),
        (
            "x86_64-unknown-linux-gnu",
            "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
            "inline-asm",
        ),
        (
            "x86_64-pc-windows-msvc",
            "e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
            "__chkstk",
        ),
    ];

    for (triple, data_layout, stack_probe) in cases {
        let target = TargetLayout::for_triple(triple).expect("an explicit target ABI row");
        assert_eq!(target.triple().as_bytes(), triple.as_bytes(), "{triple}");
        assert_eq!(
            target.data_layout().as_bytes(),
            data_layout.as_bytes(),
            "{triple}"
        );
        assert_eq!(target.stack_probe(), stack_probe, "{triple}");
        assert_eq!(target.address_index_max(), i64::MAX as u64, "{triple}");
        assert_eq!(target.runtime_allocation_max(), i64::MAX as u64, "{triple}");
    }
}

#[test]
fn explicit_target_selection_is_exact_and_fail_closed() {
    for unsupported in [
        "",
        "x86_64-pc-windows-gnu",
        "x86_64-pc-windows-msvc19.33.0",
        "aarch64-pc-windows-msvc",
        "x86_64-unknown-windows-msvc",
        "X86_64-pc-windows-msvc",
        "x86_64-pc-windows-msvc ",
    ] {
        assert_eq!(
            TargetLayout::for_triple(unsupported),
            Err(TargetLayoutFailure::UnsupportedHost),
            "{unsupported:?}"
        );
    }
}

#[test]
fn host_selection_uses_one_exact_explicit_target_row() {
    let host = TargetLayout::host().expect("the test host is supported");
    assert_eq!(TargetLayout::for_triple(host.triple()), Ok(host));
}

#[test]
fn linked_library_substitution_preserves_argument_access() {
    // Substitution happens at link time; the WF module and launcher are identical.
    assert_eq!(
        super::compile(READS_ITS_ARGUMENTS),
        emit_for_deterministic_target(READS_ITS_ARGUMENTS)
    );

    // And it observes the same invocation vector: the deterministic target
    // scripts host objects, never the arguments the harness already controls
    // exactly.
    let run = run_on_deterministic_host(READS_ITS_ARGUMENTS, &HostScript::new(), &[b"a", b"b"]);
    assert_eq!(run.output.status.code(), Some(3));
    assert_eq!(run.attempts("close"), 1);
    assert_eq!(run.attempts("open"), 0);
}

#[test]
fn an_explicit_close_that_fails_is_attempted_once_and_never_retried() {
    // The ordinary close implementation attempts once even on EINTR.
    // The caller explicitly discards its Result; an implicit drop does nothing.
    let run = run_on_deterministic_host(
        RELEASES_ONE_DIRECTORY,
        &HostScript::new().closes(&[HostOutcome::Fail(HostError::Interrupted)]),
        &[],
    );

    assert_eq!(
        run.attempts("close"),
        1,
        "an interrupted close is never retried; trace was {:?}",
        run.trace()
    );
    // The descriptor closed is the one the scripted directory open produced,
    // and the attempt reported a failure: the value itself is the selected
    // host's own `EINTR`, not a number this test fixes.
    assert!(
        run.trace()
            .lines()
            .any(|line| line.starts_with("wf_test close fd=") && line.ends_with("outcome=error"))
    );
    // This caller discards the close Result and keeps its selected exit status.
    assert_eq!(run.output.status.code(), Some(0));
}

#[test]
fn an_explicit_close_that_succeeds_is_also_exactly_one_attempt() {
    // The control for the case above: the single attempt is a property of the
    // release, not of the failure.
    let run = run_on_deterministic_host(
        RELEASES_ONE_DIRECTORY,
        &HostScript::new().closes(&[HostOutcome::Succeed]),
        &[],
    );

    assert_eq!(run.attempts("close"), 1);
    assert!(
        run.trace()
            .lines()
            .any(|line| line.starts_with("wf_test close fd=")
                && !line.starts_with("wf_test close fd=42 ")
                && line.ends_with("outcome=ok"))
    );
    assert_eq!(run.output.status.code(), Some(0));
}

#[test]
fn an_inspection_error_survives_a_failed_provisional_close() {
    // The open implementation preserves the inspection error while closing
    // its provisional descriptor once. The ordinary caller sees that error.
    let source = opens_one_file(
        &[(
            "DeviceFailure",
            "if o == 4_u8 {\n  let narrowed = cvt.checked::<u32, u8>(c);\n  match narrowed {\n    Ok(value: code) => {\n      return std::process::exit_status(code: code);\n    }\n    Err(error: overflowed) => {\n      return std::process::exit_status(code: 250_u8);\n    }\n  }\n} else {\n  return std::process::exit_status(code: 251_u8);\n}",
        )],
        "return std::process::exit_status(code: 199_u8);",
    );
    let run = run_on_deterministic_host(
        source.as_bytes(),
        &HostScript::new()
            .file(b"x")
            .file_status(HostFileStatus::Fail(HostError::DeviceFailure))
            .closes(&[
                HostOutcome::Fail(HostError::Interrupted),
                HostOutcome::Succeed,
            ]),
        &[],
    );

    let inspection_code = run
        .trace()
        .lines()
        .find_map(|line| {
            line.strip_prefix("wf_test fstat fd=42 outcome=error code=")
                .and_then(|code| code.parse::<i32>().ok())
        })
        .expect("the failing inspection reports its native code");
    assert_eq!(run.output.status.code(), Some(inspection_code));
    assert_eq!(run.attempts("fstat"), 1);
    assert_eq!(run.attempts("close"), 2);
    assert!(run.trace().contains("wf_test fstat fd=42 outcome=error"));
    assert!(run.trace().contains("wf_test close fd=42 outcome=error"));
    assert!(
        run.trace()
            .lines()
            .any(|line| line.starts_with("wf_test close fd=")
                && !line.starts_with("wf_test close fd=42 ")
                && line.ends_with("outcome=ok"))
    );
}

#[test]
fn a_nonregular_result_survives_a_failed_provisional_close() {
    // The linked open implementation classifies the provisional descriptor
    // and preserves that outcome if its cleanup close fails.
    let source = opens_one_file(
        &[(
            "IsDirectory",
            "if c == 0_u32 {\n  if o == 0_u8 {\n    return std::process::exit_status(code: 23_u8);\n  } else {\n    return std::process::exit_status(code: 24_u8);\n  }\n} else {\n  return std::process::exit_status(code: 25_u8);\n}",
        )],
        "return std::process::exit_status(code: 199_u8);",
    );
    let run = run_on_deterministic_host(
        source.as_bytes(),
        &HostScript::new()
            .file(b"x")
            .file_status(HostFileStatus::Directory)
            .closes(&[
                HostOutcome::Fail(HostError::Interrupted),
                HostOutcome::Succeed,
            ]),
        &[],
    );

    assert_eq!(
        run.output.status.code(),
        Some(23),
        "trace was {:?}",
        run.trace()
    );
    assert_eq!(run.attempts("fstat"), 1);
    assert_eq!(run.attempts("close"), 2);
    assert!(run.trace().contains("wf_test fstat fd=42 outcome=ok"));
    assert!(run.trace().contains("wf_test close fd=42 outcome=error"));
    assert!(
        run.trace()
            .lines()
            .any(|line| line.starts_with("wf_test close fd=")
                && !line.starts_with("wf_test close fd=42 ")
                && line.ends_with("outcome=ok"))
    );
}

#[test]
fn substituting_linked_closes_keeps_one_ordinary_call_in_optimized_ir() {
    // Native syscall substitution cannot alter the WF call ABI.
    let optimized = host_optimized_module(&emit_for_deterministic_target(RELEASES_ONE_DIRECTORY));
    // The optimized WF body calls its ordinary declaration; linked internals
    // are neither copied into this module nor selected by the compiler.
    assert!(optimized.contains("@wf_std.fs.close_directory("));
    assert!(!optimized.contains("@wf_test_close_submit"));
    assert!(!optimized.contains("@malloc"));
}

#[test]
fn a_mid_stream_read_failure_stops_the_drain_after_the_bytes_it_delivered() {
    // A file that reads normally and then fails part way through cannot be
    // arranged on a real filesystem at a chosen call, so this is the
    // deterministic target's case. The first attempt delivers three bytes and
    // the second reports a device failure; the drain must observe
    // `Ok(3)` then `Err(ReadFailed(...))`, not a silent end of input.
    let source = std::str::from_utf8(CHUNKED_READ)
        .expect("source is UTF-8")
        .replace(
            "return std::process::exit_status(code: 202_u8);",
            "let first = bytes[0_u64];
                let second = bytes[1_u64];
                let third = bytes[2_u64];
                if first != 97_u8 {
                  return std::process::exit_status(code: 206_u8);
                }
                if second != 98_u8 {
                  return std::process::exit_status(code: 207_u8);
                }
                if third != 99_u8 {
                  return std::process::exit_status(code: 208_u8);
                }
                return std::process::exit_status(code: 202_u8);",
        );
    let run = run_on_deterministic_host(
        source.as_bytes(),
        &HostScript::new().file(b"abcdefgh").reads(&[
            HostOutcome::Succeed,
            HostOutcome::Fail(HostError::DeviceFailure),
        ]),
        &[b"eight.txt"],
    );

    // 202 is the program's own `ReadFailed` status after inspecting every
    // destination byte. The failed call preserved the preceding "abc", and
    // its error was not reported as the end of input.
    assert_eq!(
        run.output.status.code(),
        Some(202),
        "trace was {:?}",
        run.trace()
    );
    // Exactly two attempts: the failing one ended the drain and nothing
    // retried it.
    assert_eq!(run.attempts("pread"), 2);
    assert!(
        run.trace()
            .contains("wf_test pread fd=42 offset=0 capacity=3 delivered=3")
    );
    assert!(
        run.trace()
            .contains("wf_test pread fd=42 offset=3 capacity=3 -> error")
    );

    // The control: the same program over the same fixture with nothing
    // scripted drains the file to its end and reports its own total.
    let clean = run_on_deterministic_host(
        source.as_bytes(),
        &HostScript::new().file(b"abcdefgh"),
        &[b"eight.txt"],
    );
    assert_eq!(clean.output.status.code(), Some(83));
    assert_eq!(clean.attempts("pread"), 4);
}

#[test]
fn read_no_progress_answers_are_internal_until_one_positioned_read_progresses() {
    let run = run_on_deterministic_host(
        CHUNKED_READ,
        &HostScript::new().file(b"abc").reads(&[
            HostOutcome::Fail(HostError::Interrupted),
            HostOutcome::Fail(HostError::WouldBlock),
            HostOutcome::Succeed,
        ]),
        &[b"three.txt"],
    );
    assert_eq!(
        run.output.status.code(),
        Some(31),
        "trace was {:?}",
        run.trace()
    );
    assert_eq!(run.trace().matches("progress=eintr").count(), 1);
    assert_eq!(run.trace().matches("wait=readable").count(), 1);
    assert_eq!(run.trace().matches("delivered=3").count(), 1);
    assert!(!run.trace().contains("-> error"));
}

#[test]
fn write_no_progress_answers_are_internal_until_one_write_progresses() {
    let run = run_on_deterministic_host(
        WRITE_PREFIX,
        &HostScript::new().writes(&[
            HostOutcome::Fail(HostError::Interrupted),
            HostOutcome::Fail(HostError::WouldBlock),
            HostOutcome::Succeed,
        ]),
        &[],
    );
    assert_eq!(
        run.output.status.code(),
        Some(3),
        "trace was {:?}",
        run.trace()
    );
    assert_eq!(run.trace().matches("progress=eintr").count(), 1);
    assert_eq!(run.trace().matches("wait=writable").count(), 1);
    assert_eq!(run.trace().matches("accepted=2").count(), 1);
    assert!(run.trace().contains("bytes=xy"));
    assert!(!run.trace().contains("-> error"));
}

#[test]
fn a_forced_short_write_reports_the_absolute_endpoint_after_the_host_prefix() {
    // A destination that accepts only part of one request is not something a
    // regular file or a pipe can be made to do on demand at a chosen call.
    // The linked `write_once` reports the first progress, so a partial
    // acceptance is `Ok(next)` with the exact absolute endpoint — never a silent
    // loop that finishes the range, and never an error.
    let run = run_on_deterministic_host(
        WRITE_PREFIX,
        // The program's first request is zero-length and issues no host
        // transfer, so the first scripted entry meets its second request of
        // two bytes.
        &HostScript::new().writes(&[HostOutcome::Accept(1)]),
        &[],
    );

    // The request starts at one, so accepting one byte reports endpoint two.
    assert_eq!(
        run.output.status.code(),
        Some(2),
        "trace was {:?}",
        run.trace()
    );
    // One request, one attempt, and exactly the accepted prefix reached the
    // sink: the unaccepted byte was not written by a retry.
    assert_eq!(run.attempts("write"), 1);
    assert!(
        run.trace()
            .contains("wf_test write fd=1 count=2 accepted=1 bytes=x")
    );

    // The control: with nothing scripted the same request is accepted whole
    // and reports endpoint three.
    let whole = run_on_deterministic_host(WRITE_PREFIX, &HostScript::new(), &[]);
    assert_eq!(whole.output.status.code(), Some(3));
    assert!(
        whole
            .trace()
            .contains("wf_test write fd=1 count=2 accepted=2 bytes=xy")
    );
}

#[test]
fn an_affine_output_drop_does_not_call_a_close() {
    // A host handle's drop is empty [PRE-2]. OutputStream is affine, so dropping it
    // performs no native close or flush. DirectoryRead is explicitly closed.
    let run = run_on_deterministic_host(
        WRITES_THEN_RELEASES_BOTH,
        &HostScript::new()
            // Only the explicit directory close can consume a scripted answer.
            .closes(&[
                HostOutcome::Fail(HostError::DeviceFailure),
                HostOutcome::Fail(HostError::DeviceFailure),
            ]),
        &[],
    );

    assert_eq!(
        run.output.status.code(),
        Some(3),
        "trace was {:?}",
        run.trace()
    );
    // The write was accepted in full and observed as such by the program.
    assert!(
        run.trace()
            .contains("wf_test write fd=1 count=3 accepted=3 bytes=ABC")
    );
    // Exactly one close attempt, and it is the `DirectoryRead`'s. Neither
    // `OutputStream` owner closed its descriptor, so the sink's close-time failure
    // is outside what any release can observe.
    assert_eq!(run.attempts("close"), 1);
    assert!(
        run.trace()
            .lines()
            .any(|line| line.starts_with("wf_test close fd=") && line.ends_with("outcome=error"))
    );
    assert!(!run.trace().contains("wf_test close fd=1 "));
    assert!(!run.trace().contains("wf_test close fd=2 "));
}

#[test]
fn the_heap_resource_record_writer_stays_native_on_the_deterministic_target() {
    // The resource-exhaustion recorder stays independent of the ordinary
    // library's substituted write implementation. Both paths remain callable.
    // The heap is the one [STOR-8] heap and a `Box` is what puts this module
    // on it; allocation is total in the source, so the record writer below is
    // the trusted base's own exhaustion path and not a source-visible arm.
    let source = br#"fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  let std::process::Inputs(args: unused_args, cwd: unused_cwd, stdout: out, stderr: unused_stderr, handles: entry_factory, stdin: unused_stdin) = move inputs;
  std::fs::close_directory(factory: &entry_factory, directory: move unused_cwd);
  let bytes = box_array_filled::<u8>(count: 1_u64, value: 65_u8);
  let ordinary_source = &bytes.inner[0_u64..1_u64];
  match std::io::write_once(factory: &entry_factory, output: &out, source: ordinary_source, start: 0_u64, end: 1_u64) {
    Ok(value: accepted) => {
    }
    Err(error: problem) => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = emit_for_deterministic_target(source);
    // The record loop reaches native write through its EINTR retry helper.
    // Both edges must remain independent of the substituted library call.
    assert!(module.contains("declare void @wf_std.io.write_once(ptr %wf.result,"));
    assert!(module.contains("declare i64 @write(i32, ptr, i64)"));
    assert!(module.contains("%written = call i64 @wf_resource_write(ptr %cursor, i64 %remaining)"));
    assert!(module.contains("%written = call i64 @write(i32 2, ptr %bytes, i64 %length)"));
    assert!(module.contains("call void @wf_resource_record_abort("));
    assert!(module.contains("call void @wf_std.io.write_once("));
    assert!(!module.contains("@wf_test_write_submit"));

    // And the native target still declares exactly one `@write` for both.
    let native = super::compile(source);
    assert_eq!(
        native.matches("declare i64 @write(i32, ptr, i64)").count(),
        1
    );
}

#[test]
fn a_host_that_accepts_nothing_reaches_source_as_write_zero() {
    assert_zero_write_outcome();
}

pub(super) fn assert_zero_write_outcome() {
    // A linked write reporting zero progress on a nonempty request produces
    // Err(WriteZero) with zero code/origin. This is actual source behavior,
    // tested by substituting the native facility rather than inspecting IR.
    let arms = class_arms(
        8,
        &[(
            "WriteZero",
            "if c == 0_u32 {\n  if o == 0_u8 {\n    return std::process::exit_status(code: 120_u8);\n  } else {\n    return std::process::exit_status(code: 121_u8);\n  }\n} else {\n  return std::process::exit_status(code: 122_u8);\n}",
        )],
        "return std::process::exit_status(code: 199_u8);",
    );
    let source = format!(
        r#"fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {{
  let std::process::Inputs(args: args, cwd: cwd, stdout: out, stderr: err, handles: factory, stdin: input) = move inputs;
  std::fs::close_directory(factory: &factory, directory: move cwd);
  let bytes = array_filled::<u8, 2>(value: 119_u8);
  let window = &bytes[0_u64..2_u64];
  match std::io::write_once(factory: &factory, output: &out, source: window, start: 0_u64, end: 2_u64) {{
    Ok(value: written) => {{
      let narrowed = cvt.checked::<u64, u8>(written);
      match narrowed {{
        Ok(value: code) => {{
          return std::process::exit_status(code: code);
        }}
        Err(error: overflowed) => {{
          return std::process::exit_status(code: 200_u8);
        }}
      }}
    }}
    Err(error: problem) => {{
      match problem {{
{arms}      }}
    }}
  }}
}}
"#
    );

    let run = run_on_deterministic_host(
        source.as_bytes(),
        &HostScript::new().writes(&[HostOutcome::Accept(0)]),
        &[],
    );
    // 120 is the program's own `WriteZero` status with both detail fields
    // observed as zero.
    assert_eq!(
        run.output.status.code(),
        Some(120),
        "trace was {:?}",
        run.trace()
    );
    // One request, one attempt: the refusal ended the operation and nothing
    // retried it inside the linked `write_once` implementation.
    assert_eq!(run.attempts("write"), 1);
    assert!(
        run.trace()
            .contains("wf_test write fd=1 count=2 accepted=0 bytes=")
    );

    // The control: with nothing scripted the same request is accepted whole
    // and the same program reports the accepted count instead of any class.
    let whole = run_on_deterministic_host(source.as_bytes(), &HostScript::new(), &[]);
    assert_eq!(whole.output.status.code(), Some(2));
    assert!(
        whole
            .trace()
            .contains("wf_test write fd=1 count=2 accepted=2 bytes=ww")
    );
}

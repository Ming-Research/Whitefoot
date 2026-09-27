//! The ordinary directory library record decoder, over native records this module
//! builds by hand.
//!
//! The linked `directory_next` body reads a
//! record the host wrote rather than a count the host returned, and the two
//! native implementations consume different records: Darwin's `struct dirent`
//! states the name's length in a field of its own, and Linux's
//! `struct linux_dirent64` states none at all and NUL-terminates the name
//! inside the extent `d_reclen` reports. The decoder must therefore be
//! exercised against names of every admitted length, against every native
//! entry kind, and against records no correct kernel produces — an extent
//! that does not advance, an extent reaching past the reported batch, a name
//! that does not fit the record carrying it. A real directory cannot be made
//! to produce the last group on demand.
//!
//! The substitution is the one `compiler/Makefile` already uses for the
//! completion harness: the file adapter reaches its family's enumeration
//! facility through a macro, and these cases define that macro to a function
//! this module writes. Nothing else changes. The bootstrap still opens the
//! real working directory, `open_directory_source` still opens a real
//! descriptor against it, and the decoder under test is the shipped emitted
//! shim reading the bytes the scripted facility left in the caller's own
//! buffer.

use super::{compile, compile_link_and_run_with};

/// What a scripted record misstates about itself, if anything.
///
/// Each names a way a facility inside the trusted computing base could
/// contradict itself. The decoder ends the walk at `abort` for every one of
/// them under its private native record contract: none is a source-visible outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecordDefect {
    /// The record is exactly what its family fixes.
    None,
    /// The record's own length is zero, so accepting it would not advance the
    /// walk.
    ZeroExtent,
    /// The record's own length reaches past the end of the batch the facility
    /// reported.
    ExtentBeyondBatch,
    /// The record's extent is too small for the name it carries: on Darwin
    /// the stated name length exceeds the extent, and on Linux no terminator
    /// lies inside it.
    NameBeyondExtent,
}

impl RecordDefect {
    /// The discriminant the generated unit switches on.
    const fn tag(self) -> u8 {
        match self {
            Self::None => 0,
            Self::ZeroExtent => 1,
            Self::ExtentBeyondBatch => 2,
            Self::NameBeyondExtent => 3,
        }
    }
}

/// One entry a scripted batch reports.
#[derive(Clone, Copy, Debug)]
struct ScriptedEntry {
    /// The name's one repeated byte, so a 255-byte name is stated rather than
    /// typed.
    byte: u8,
    /// The name's byte length.
    length: u16,
    /// The native entry-type discriminant. `DT_REG`, `DT_DIR`, `DT_LNK`, and
    /// `DT_UNKNOWN` carry the same four values on both qualified families.
    native_kind: u8,
    defect: RecordDefect,
}

impl ScriptedEntry {
    const fn regular(byte: u8, length: u16) -> Self {
        Self::kinded(byte, length, 8)
    }

    const fn kinded(byte: u8, length: u16, native_kind: u8) -> Self {
        Self {
            byte,
            length,
            native_kind,
            defect: RecordDefect::None,
        }
    }

    const fn malformed(byte: u8, length: u16, defect: RecordDefect) -> Self {
        Self {
            byte,
            length,
            native_kind: 8,
            defect,
        }
    }

    /// The portable record this entry must normalize into: one kind byte, one
    /// little-endian `u16` name length, then exactly that many name bytes
    /// under the ordinary directory library contract.
    fn portable(self) -> Vec<u8> {
        let kind = match self.native_kind {
            0 => 0u8,
            8 => 1,
            4 => 2,
            10 => 3,
            _ => 4,
        };
        let mut record = vec![
            kind,
            (self.length & 0xff) as u8,
            ((self.length >> 8) & 0xff) as u8,
        ];
        record.extend(std::iter::repeat_n(self.byte, usize::from(self.length)));
        record
    }
}

/// The native record layout of the selected host family, as the generated
/// unit's own constants.
///
/// These are the offsets the native library implements for the same
/// family. Stating them independently is deliberate: a change on one side
/// that leaves the other alone shows up as a decoded-bytes mismatch rather
/// than as agreement between two copies of one mistake.
const fn record_layout() -> &'static str {
    if cfg!(target_os = "macos") {
        "#define WF_RECORD_LENGTH_OFFSET 16\n\
         #define WF_RECORD_NAME_LENGTH_OFFSET 18\n\
         #define WF_RECORD_TYPE_OFFSET 20\n\
         #define WF_RECORD_NAME_OFFSET 21\n"
    } else {
        "#define WF_RECORD_LENGTH_OFFSET 16\n\
         #define WF_RECORD_TYPE_OFFSET 18\n\
         #define WF_RECORD_NAME_OFFSET 19\n"
    }
}

/// The longest single component the selected target's enumeration row admits
/// under the ordinary directory library contract.
const fn component_limit() -> u16 {
    if cfg!(target_os = "macos") { 1023 } else { 255 }
}

/// The host translation unit answering the enumeration facility with one
/// scripted batch.
fn scripted_batch_unit(entries: &[ScriptedEntry]) -> String {
    let table = entries
        .iter()
        .map(|entry| {
            format!(
                "    {{ {byte}u, {length}u, {kind}u, {defect}u }},\n",
                byte = entry.byte,
                length = entry.length,
                kind = entry.native_kind,
                defect = entry.defect.tag(),
            )
        })
        .collect::<String>();
    // Darwin's facility takes the base-position cell it writes; Linux's takes
    // no such argument, exactly as the file adapter's own two calls do.
    let (signature, position) = if cfg!(target_os = "macos") {
        (
            "ssize_t wf_test_directory_batch(int descriptor, void *buffer, size_t count, \
             int64_t *position)",
            "    if (position != NULL) {\n        *position = 0;\n    }\n",
        )
    } else {
        (
            "ssize_t wf_test_directory_batch(int descriptor, void *buffer, size_t count)",
            "",
        )
    };
    let name_length_store = if cfg!(target_os = "macos") {
        "        wf_store16(\n            \
         cursor + offset + WF_RECORD_NAME_LENGTH_OFFSET,\n            \
         (uint16_t)(defect == 3u ? name_length + extent : name_length)\n        \
         );\n"
    } else {
        // The Linux record states no length. Its truncation defect is the
        // missing terminator, so the whole name area is filled below.
        ""
    };
    format!(
        "#include <stddef.h>\n\
         #include <stdint.h>\n\
         #include <string.h>\n\
         #include <sys/types.h>\n\n\
         {layout}\n\
         struct wf_scripted_entry {{\n    \
         unsigned char byte;\n    \
         unsigned short name_length;\n    \
         unsigned char native_kind;\n    \
         unsigned char defect;\n\
         }};\n\n\
         static const struct wf_scripted_entry wf_scripted[] = {{\n\
         {table}    \
         {{ 0u, 0u, 0u, 0u }}\n\
         }};\n\n\
         static const size_t wf_scripted_count = {count}u;\n\n\
         /* One record holds its header, the name, the terminator, and the\n   \
         padding both families round a record up to. */\n\
         static size_t wf_record_extent(size_t name_length) {{\n    \
         size_t needed = (size_t)WF_RECORD_NAME_OFFSET + name_length + 1u;\n    \
         size_t remainder = needed % 8u;\n    \
         return remainder == 0u ? needed : needed + (8u - remainder);\n\
         }}\n\n\
         static void wf_store16(unsigned char *at, uint16_t value) {{\n    \
         at[0] = (unsigned char)(value & 0xffu);\n    \
         at[1] = (unsigned char)((value >> 8) & 0xffu);\n\
         }}\n\n\
         {signature} {{\n    \
         unsigned char *cursor = (unsigned char *)buffer;\n    \
         size_t offset = 0u;\n    \
         size_t index;\n    \
         (void)descriptor;\n\
         {position}    \
         for (index = 0u; index < wf_scripted_count; ++index) {{\n        \
         size_t name_length = (size_t)wf_scripted[index].name_length;\n        \
         size_t extent = wf_record_extent(name_length);\n        \
         unsigned char defect = wf_scripted[index].defect;\n        \
         size_t stated = extent;\n        \
         if (offset + extent > count) {{\n            \
         break;\n        \
         }}\n        \
         memset(cursor + offset, 0, extent);\n        \
         if (defect == 1u) {{\n            \
         stated = 0u;\n        \
         }} else if (defect == 2u) {{\n            \
         stated = extent + count;\n        \
         }}\n        \
         wf_store16(cursor + offset + WF_RECORD_LENGTH_OFFSET, (uint16_t)stated);\n        \
         cursor[offset + WF_RECORD_TYPE_OFFSET] = wf_scripted[index].native_kind;\n\
         {name_length_store}        \
         memset(\n            \
         cursor + offset + WF_RECORD_NAME_OFFSET,\n            \
         wf_scripted[index].byte,\n            \
         defect == 3u ? extent - (size_t)WF_RECORD_NAME_OFFSET : name_length\n        \
         );\n        \
         offset += extent;\n    \
         }}\n    \
         return (ssize_t)offset;\n\
         }}\n",
        layout = record_layout(),
        count = entries.len(),
    )
}

/// The definitions that point the file adapter's enumeration facility at the
/// scripted batch above.
///
/// Both family macros are defined because only one of them exists in any one
/// build, and naming both keeps this from becoming a second place a family has
/// to be added to.
fn scripted_facility_defines() -> Vec<String> {
    vec![
        "WF_COMPLETION_GETDIRENTRIES64=wf_test_directory_batch".to_owned(),
        "WF_COMPLETION_GETDENTS64=wf_test_directory_batch".to_owned(),
    ]
}

/// Publishes one enumeration batch's portable prefix on standard output.
///
/// The program is ordinary source: it names no target record and reads only
/// the portable form under the ordinary directory library contract fixes.
const PUBLISH_ONE_BATCH: &[u8] = br#"fn exercise(cwd: &std::fs::DirectoryRead, out: &std::io::OutputStream, files: &std::io::HandleFactory) -> status: std::process::ExitStatus reads(cwd), writes(out), writes(files) {
  let entries = array_filled::<u8, 4096>(value: 0_u8);
  let available = 0_u64;
  match std::fs::open_directory_source(factory: files, directory: cwd) {
    Ok(value: list) => {
      let window = &entries[0_u64..4096_u64];
      let (copied, endpoint, reported) = std::fs::directory_next(source: &list, destination: window, start: 0_u64, end: 4096_u64);
      match copied {
        Ok(value: done) => {
          set available = endpoint;
        }
        Err(error: stop) => {
          match stop {
            ListEnd() => {
              std::fs::close_directory_source(factory: files, source: move list);
              return std::process::exit_status(code: 3_u8);
            }
            ListFailed(error: problem) => {
              std::fs::close_directory_source(factory: files, source: move list);
              return std::process::exit_status(code: 4_u8);
            }
          }
        }
      }
      match std::io::write_once(factory: files, output: out, source: window, start: 0_u64, end: available) {
        Ok(value: written) => {
        }
        Err(error: problem) => {
          std::fs::close_directory_source(factory: files, source: move list);
          return std::process::exit_status(code: 2_u8);
        }
      }
      std::fs::close_directory_source(factory: files, source: move list);
    }
    Err(error: problem) => {
      return std::process::exit_status(code: 5_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  let std::process::Inputs(args: unused_args, cwd: cwd, stdout: out, stderr: unused_stderr, handles: files, stdin: unused_stdin) = move inputs;
  let outcome = exercise(cwd: &cwd, out: &out, files: &files);
  std::fs::close_directory(factory: &files, directory: move cwd);
  return move outcome;
}
"#;

/// Runs the publishing program against one scripted batch.
fn published_records(entries: &[ScriptedEntry]) -> std::process::Output {
    let llvm = compile(PUBLISH_ONE_BATCH);
    compile_link_and_run_with(
        &llvm,
        Some(&scripted_batch_unit(entries)),
        &scripted_facility_defines(),
        &[],
    )
}

/// The portable bytes a scripted batch must normalize into.
fn expected_records(entries: &[ScriptedEntry]) -> Vec<u8> {
    entries.iter().flat_map(|entry| entry.portable()).collect()
}

/// Names of every admitted length decode exactly.
///
/// The lengths bracket both families' record arithmetic — one byte, the
/// alignment boundary either side, and the longest component the selected
/// target admits. On the Linux family the longest is 255 bytes and every one
/// of these lengths is derived by the bounded scan rather than read from a
/// field, which is the whole difference between the two rows.
#[test]
fn names_of_every_admitted_length_decode_to_the_portable_record() {
    let longest = component_limit();
    let entries: Vec<ScriptedEntry> = [1u16, 2, 3, 4, 7, 8, 9, 16, 63, 64, 255, longest]
        .into_iter()
        .filter(|length| *length <= longest)
        .enumerate()
        .map(|(position, length)| {
            ScriptedEntry::regular(
                b'a' + u8::try_from(position % 26).expect("a small index"),
                length,
            )
        })
        .collect();
    let output = published_records(&entries);
    assert!(
        output.status.success(),
        "enumeration exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, expected_records(&entries));
}

/// The closed kind set, including the value meaning the target classified the
/// entry as nothing more specific and a value belonging to none of the four
/// named classes under the ordinary directory library contract.
#[test]
fn every_native_entry_kind_maps_into_the_closed_portable_set() {
    // `DT_REG`, `DT_DIR`, `DT_LNK`, `DT_UNKNOWN`, and `DT_FIFO`, which has no
    // portable class of its own and is therefore `other`.
    let entries = [
        ScriptedEntry::kinded(b'r', 6, 8),
        ScriptedEntry::kinded(b'd', 6, 4),
        ScriptedEntry::kinded(b'l', 6, 10),
        ScriptedEntry::kinded(b'u', 6, 0),
        ScriptedEntry::kinded(b'o', 6, 1),
    ];
    let output = published_records(&entries);
    assert!(
        output.status.success(),
        "enumeration exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, expected_records(&entries));
    let kinds: Vec<u8> = output.stdout.chunks(9).map(|record| record[0]).collect();
    assert_eq!(kinds, vec![1, 2, 3, 0, 4]);
}

/// A batch that runs to the end of the caller's range decodes every record it
/// holds and no more.
///
/// The portable record is always shorter than the native one that produced
/// it, so a native batch filling the range normalizes into a strictly shorter
/// prefix of the same range and never overruns it.
#[test]
fn a_batch_that_fills_the_range_decodes_every_record_it_holds() {
    // More records than any 4096-byte range can hold on either family, so
    // the batch always stops at the range and the case always tests a
    // range-filling batch rather than a batch that happened to fit.
    let entries: Vec<ScriptedEntry> = (0..200u16)
        .map(|position| {
            ScriptedEntry::regular(
                b'a' + u8::try_from(position % 26).expect("a small index"),
                32 + position % 17,
            )
        })
        .collect();
    let output = published_records(&entries);
    assert!(
        output.status.success(),
        "enumeration exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = expected_records(&entries);
    assert!(!output.stdout.is_empty());
    assert!(
        expected.starts_with(&output.stdout),
        "the decoded prefix must be the portable form of the records the batch held"
    );
    // The scripted facility stops at the caller's range, so the batch holds a
    // proper prefix of the entries above; the case is meaningless if it fits.
    assert!(
        output.stdout.len() < expected.len(),
        "the scripted batch must be larger than the caller's range"
    );
}

/// A batch reporting no bytes at all is the end of the enumeration, never an
/// empty record (ordinary native library).
#[test]
fn an_empty_batch_is_the_end_of_the_enumeration() {
    let output = published_records(&[]);
    assert_eq!(
        output.status.code(),
        Some(3),
        "an empty batch must reach ListEnd: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}

/// A record no correct facility produces ends the walk at the
/// trusted-computing-base defect arm rather than producing a source-visible
/// outcome (private native record invariant).
#[test]
fn a_record_that_contradicts_its_family_layout_ends_the_walk() {
    for defect in [
        RecordDefect::ZeroExtent,
        RecordDefect::ExtentBeyondBatch,
        RecordDefect::NameBeyondExtent,
    ] {
        let entries = [
            ScriptedEntry::regular(b'a', 5),
            ScriptedEntry::malformed(b'b', 6, defect),
        ];
        let output = published_records(&entries);
        assert!(
            !output.status.success(),
            "{defect:?} must not produce a successful enumeration"
        );
        assert_eq!(
            output.status.code(),
            None,
            "{defect:?} must reach the defect arm's abort rather than a status"
        );
    }
}

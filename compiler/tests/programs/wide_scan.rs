//! Typed-outcome oracle for proof-gated wide-probe lowering.
//!
//! The program below carries five byte-walk loops of the recognized shape.
//! The equivalence walks pin the exact per-byte results (needle positions at
//! every probe-lane boundary, an absent needle, and a bound below the run's
//! length). The argument-selected boundary walks deliberately use loop bounds
//! past their runs, but express exhaustion as distinct exit statuses: at the
//! first byte of an empty run, and one past the last byte at an offset inside
//! a would-be wide stride, with every pre-failure published effect identical
//! to the scalar reference. Expectations are fixed input/output/status
//! triples;
//! the compiler has no optimizer-fact channel, so this single ordinary
//! mode is the facts-off mode.
//!
//! **THE FIXTURE IS NO LONGER ON `buffer<T>`, AND THAT IS WHAT THIS ORACLE
//! NOW DEMANDS OF THE PROBE.** The module doc used to say the fixture was left
//! on `buffer<T>` deliberately, because the wide probe was a buffer-only
//! lowering at both ends: the recognizer matched a
//! `CheckedExpression::BufferIndex` walk and nothing else, and the emitter
//! refused any operand whose IR type was not `IrType::Buffer`. The v0.60 port
//! migrated the fixture anyway. The three equivalence walks below now walk a
//! constant-capacity `Array<u8, N>` and read its length as the readonly field
//! `data.len` [TYPE-9, MSR-1, OP-15], and the two boundary walks index a boxed
//! `Array<u8>` and an `Array<u8, 37>`. The `count == 3` assertion below is
//! therefore a demand on the probe rather than a description of it, and it
//! stays exactly as written: while the recognizer still matches only a buffer
//! walk it finds no wide load at all and this test fails, and it passes again
//! when the recognizer and the emitter reach `Array` and window walks.
//!
//! Extending the probe is not a rename, and the shapes differ in what its
//! guard has to read. An `Array<T, N>` is a contiguous run whose `len` is the
//! type constant N and whose `cap` is absent [MSR-1], so `base + index` is
//! always the right address and the guard reads one constant; an `Array<T>`
//! is the same run with a runtime length. A `Slots` or a `Ring` is a
//! *window* -- `P.len` slots beginning at the origin `P.head` taken modulo
//! `P.cap`, under the map `i |-> (origin + i) mod P.cap` [MSR-1] -- so
//! `base + index` is the right address only where the origin is proved
//! identically zero: always for a `Slots`, whose row gives `head` as
//! *absent*, and never for a `Ring` after a front operation, `head` being the
//! one *bounded* cell of that table. A range reference `&[T]` carries one
//! base and the range's own `len` and is the `Array` case again. That is a
//! backend change with its own proof obligation and its own cases
//! (compiler/wide-probe-lowering), not part of the port that moved the
//! fixture.
//!
//! Publication uses a range reference over the retained fixture [REF-4].

use super::support::{build_program, compile_sources, fixture_directory};

const ORACLE: &[u8] = br#"fn opaque_length(n: u64) -> result: u64 pure contract {
  requires n <= 0_u64;
  ensures result <= 0_u64;
} {
  return n;
}

fn publish_all(factory: &std::io::HandleFactory, output: &std::io::OutputStream, source: &[u8], length: u64) -> result: Result<unit, std::io::IoError> reads(source), writes(factory), writes(output) contract {
  define source_length = deref(source).len;
  requires length <= source_length;
} {
  doc "Publishes one prefix of the source range, reattempting until the host has accepted every byte or refused it.";
  let sent = 0_u64;
  loop @publish {
    let pending = sent < length;
    if pending {
    } else {
      break @publish;
    }
    match std::io::write_once(factory: factory, output: output, source: source, start: sent, end: length) {
      Ok(value: accepted) => {
        set sent = accepted;
      }
      Err(error: problem) => {
        return Err<unit, std::io::IoError>(error: problem);
      }
    }
  }
  return Ok<unit, std::io::IoError>(value: unit);
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  doc "Runs three equivalence byte walks, publishes their recorded positions, then runs one argument-selected boundary walk with a typed exhaustion status.";
  let std::process::Inputs(args: args, cwd: unused_cwd, stdout: out, stderr: unused_err, handles: factory, stdin: unused_in) = move inputs;
  std::fs::close_directory(factory: &factory, directory: move unused_cwd);
  let selector = 111_u8;
  let choice = array_filled::<u8, 8>(value: 0_u8);
  let chosen = 0_u64;
  match std::text::arg_get(args: &args, position: 1_u64) {
    Ok(value: text) => {
      let choice_window = &choice[0_u64..8_u64];
      match std::text::host_copy_bytes(value: &text, destination: choice_window, start: 0_u64, end: 8_u64) {
        Ok(value: copied) => {
          set chosen = copied;
        }
        Err(error: too_small) => {
        }
      }
    }
    Err(error: absent) => {
    }
  }
  if chosen >= 1_u64 {
    set selector = choice[0_u64];
  }
  let data = array_filled::<u8, 37>(value: 97_u8);
  set data[0_u64] = 88_u8;
  set data[1_u64] = 88_u8;
  set data[15_u64] = 88_u8;
  set data[16_u64] = 88_u8;
  set data[17_u64] = 88_u8;
  set data[31_u64] = 10_u8;
  set data[36_u64] = 88_u8;
  let found = array_filled::<u8, 64>(value: 0_u8);
  let count = 0_u64;
  let mark = 88_u8;
  let stop = data.len;
  let cursor = 0_u64;
  loop @first_walk {
    let done = cursor >= stop;
    if done {
      break @first_walk;
    }
    let byte = data[cursor];
    let newline = byte == 10_u8;
    if newline {
      match cvt.checked::<u64, u8>(cursor) {
        Ok(value: narrow) => {
          let first_newline_ok = count < 64_u64;
          if first_newline_ok {
            set found[count] = narrow;
            set count = count +wrap 1_u64;
          } else {
            return std::process::exit_status(code: 6_u8);
          }
        }
        Err(error: wide_position) => {
        }
      }
    }
    let lead = byte == mark;
    if lead {
      match cvt.checked::<u64, u8>(cursor) {
        Ok(value: narrow_lead) => {
          let first_lead_ok = count < 64_u64;
          if first_lead_ok {
            set found[count] = narrow_lead;
            set count = count +wrap 1_u64;
          } else {
            return std::process::exit_status(code: 6_u8);
          }
        }
        Err(error: wide_lead) => {
        }
      }
    }
    set cursor = cursor +wrap 1_u64;
  }
  let first_sentinel_ok = count < 64_u64;
  if first_sentinel_ok {
    set found[count] = 200_u8;
    set count = count +wrap 1_u64;
  } else {
    return std::process::exit_status(code: 6_u8);
  }
  let blank = array_filled::<u8, 40>(value: 97_u8);
  let blank_stop = blank.len;
  let blank_cursor = 0_u64;
  loop @second_walk {
    let blank_done = blank_cursor >= blank_stop;
    if blank_done {
      break @second_walk;
    }
    let blank_byte = blank[blank_cursor];
    let blank_newline = blank_byte == 10_u8;
    if blank_newline {
      return std::process::exit_status(code: 4_u8);
    }
    let blank_lead = blank_byte == mark;
    if blank_lead {
      return std::process::exit_status(code: 5_u8);
    }
    set blank_cursor = blank_cursor +wrap 1_u64;
  }
  let second_sentinel_ok = count < 64_u64;
  if second_sentinel_ok {
    set found[count] = 201_u8;
    set count = count +wrap 1_u64;
  } else {
    return std::process::exit_status(code: 6_u8);
  }
  let short_stop = 20_u64;
  let short_cursor = 0_u64;
  loop @third_walk {
    let short_done = short_cursor >= short_stop;
    if short_done {
      break @third_walk;
    }
    let short_byte = data[short_cursor];
    let short_newline = short_byte == 10_u8;
    if short_newline {
      match cvt.checked::<u64, u8>(short_cursor) {
        Ok(value: short_narrow) => {
          let short_newline_ok = count < 64_u64;
          if short_newline_ok {
            set found[count] = short_narrow;
            set count = count +wrap 1_u64;
          } else {
            return std::process::exit_status(code: 6_u8);
          }
        }
        Err(error: short_wide) => {
        }
      }
    }
    let short_lead = short_byte == mark;
    if short_lead {
      match cvt.checked::<u64, u8>(short_cursor) {
        Ok(value: short_narrow_lead) => {
          let short_lead_ok = count < 64_u64;
          if short_lead_ok {
            set found[count] = short_narrow_lead;
            set count = count +wrap 1_u64;
          } else {
            return std::process::exit_status(code: 6_u8);
          }
        }
        Err(error: short_wide_lead) => {
        }
      }
    }
    set short_cursor = short_cursor +wrap 1_u64;
  }
  let third_sentinel_ok = count < 64_u64;
  if third_sentinel_ok {
    set found[count] = 202_u8;
    set count = count +wrap 1_u64;
  } else {
    return std::process::exit_status(code: 6_u8);
  }
  let phase_room = found.len;
  let phase_fits = count <= phase_room;
  if phase_fits {
    let phase_window = &found[0_u64..64_u64];
    match publish_all(factory: &factory, output: &out, source: phase_window, length: count) {
      Ok(value: published) => {
      }
      Err(error: problem) => {
      }
    }
  }
  if selector == 102_u8 {
    let empty_length = opaque_length(n: 0_u64);
    let empty = box_array_filled::<u8>(count: empty_length, value: 0_u8);
    let empty_room = empty.inner.len;
    let empty_bound = 5_u64;
    let empty_cursor = 0_u64;
    loop @empty_walk {
      let empty_done = empty_cursor >= empty_bound;
      if empty_done {
        break @empty_walk;
      }
      let empty_walk_ok = empty_cursor < empty_room;
      if empty_walk_ok {
        let empty_byte = empty.inner[empty_cursor];
        let empty_newline = empty_byte == 10_u8;
        if empty_newline {
        }
      } else {
        return std::process::exit_status(code: 2_u8);
      }
      set empty_cursor = empty_cursor +wrap 1_u64;
    }
  }
  if selector == 109_u8 {
    let field = array_filled::<u8, 37>(value: 97_u8);
    set field[21_u64] = 88_u8;
    set field[36_u64] = 89_u8;
    let scratch = array_filled::<u8, 1>(value: 0_u8);
    let field_room = field.len;
    let wall = 64_u64;
    let probe = 0_u64;
    loop @bounded_walk {
      let walk_done = probe >= wall;
      if walk_done {
        break @bounded_walk;
      }
      let walk_in_range = probe < field_room;
      if walk_in_range {
        let selected_byte = field[probe];
        let selected_lead = selected_byte == mark;
        if selected_lead {
          set scratch[0_u64] = 88_u8;
          let lead_window = &scratch[0_u64..1_u64];
          match publish_all(factory: &factory, output: &out, source: lead_window, length: 1_u64) {
            Ok(value: lead_published) => {
            }
            Err(error: lead_problem) => {
            }
          }
        }
        let selected_tail = selected_byte == 89_u8;
        if selected_tail {
          set scratch[0_u64] = 89_u8;
          let tail_window = &scratch[0_u64..1_u64];
          match publish_all(factory: &factory, output: &out, source: tail_window, length: 1_u64) {
            Ok(value: tail_published) => {
            }
            Err(error: tail_problem) => {
            }
          }
        }
      } else {
        return std::process::exit_status(code: 3_u8);
      }
      set probe = probe +wrap 1_u64;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

/// The three equivalence walks' exact published bytes: needle positions
/// 0, 1, 15, 16, 17, the newline at 31, and the last byte 36; nothing
/// from the absent-needle walk; positions below the short bound 20; one
/// marker after each walk.
const PHASE_ONE: &[u8] = &[0, 1, 15, 16, 17, 31, 36, 200, 201, 0, 1, 15, 16, 17, 202];

#[test]
fn wide_probe_walks_keep_exact_results_and_typed_boundary_failures() {
    let llvm = compile_sources(&[("wide_scan.wf", ORACLE)]);
    // The three equivalence walks keep the wide probe: their explicit
    // capacity checks sit inside hit arms, off the skip path. Every guarded
    // subscript is statically discharged from its true-edge fact. The two
    // boundary walks return typed exhaustion statuses, so the probe correctly
    // leaves them scalar [OP-4].
    assert_eq!(
        llvm.matches("load <16 x i8>").count(),
        3,
        "the equivalence walks carry the wide probe; the status-returning boundary walks stay scalar"
    );
    let program = build_program(&llvm);
    let directory = fixture_directory();

    let ok = program.run(directory.path(), &[]);
    assert!(ok.status.success());
    assert_eq!(ok.stdout, PHASE_ONE);
    assert!(ok.stderr.is_empty());

    let first = program.run(directory.path(), &[b"first"]);
    assert_eq!(first.status.code(), Some(2));
    assert_eq!(first.stdout, PHASE_ONE);
    assert!(first.stderr.is_empty());

    let mid = program.run(directory.path(), &[b"mid"]);
    assert_eq!(mid.status.code(), Some(3));
    let expected: Vec<u8> = PHASE_ONE.iter().copied().chain([88_u8, 89_u8]).collect();
    assert_eq!(
        mid.stdout, expected,
        "every pre-failure hit must publish exactly as the scalar walk would"
    );
    assert!(mid.stderr.is_empty());
}

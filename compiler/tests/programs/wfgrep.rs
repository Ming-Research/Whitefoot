//! End-to-end evidence for the recursive search in `tests/programs/wfgrep.wf`.
//!
//! wfgrep takes a pattern and one search root, walks the tree with the
//! ordinary prelude enumeration functions, opens each regular file by its
//! enumerated name with `open_file`, reads it, and
//! publishes `PATH:LINE:TEXT` for every matching line.
//!
//! Two oracles check it. The first is a trusted reference search written
//! here rather than a host `grep`: the two grep families on the supported
//! hosts disagree about unterminated final lines and about patterns that are
//! not valid text, and wfgrep's published bytes must be compared against one
//! fixed contract, not against whichever tool the host installs. The second
//! is the host's own `grep -rn` over the same fixture tree, on the fixture
//! shape where both families agree — that is the cross-check the plan item
//! asks for, and it is what establishes that the walk reaches the same files
//! a real recursive searcher reaches.

use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Command;

use whitefoot::FragmentGranularity;

use super::support::{
    CompiledProgram, build_program, build_program_from_fragments, close_path, compile_program,
    fixture_directory, reopen_path,
};

/// The initial read window length in `tests/programs/wfgrep.wf`.
///
/// The corpus needs it to build a match that straddles a read boundary and
/// lines longer than the window; nothing in the program's contract exposes it.
///
/// The window is a heap-owned `Box<Slots<u8>>` that doubles whenever one
/// incomplete line fills it; this length fixes the first read boundary.
const BUFFER_LENGTH: usize = 4096;

/// The output batch length in `tests/programs/wfgrep.wf`: a record longer
/// than this is published in pieces rather than through the batch.
const BATCH_LENGTH: usize = 8192;

/// The entry count the walk once kept per directory before it silently
/// dropped the rest in the host's enumeration order.
const FORMER_ENTRY_CAP: usize = 64;

/// One emitted module shared by every case in this module.
///
/// Entailment over the nested walk and matcher is still the dominant compile
/// cost, so the module is produced once and every case reads it. Isolation
/// lives in each run's own fixture directory, never in the artifact.
fn wfgrep_module() -> &'static str {
    static MODULE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MODULE.get_or_init(|| compile_program("wfgrep.wf"))
}

fn wfgrep() -> &'static CompiledProgram {
    static PROGRAM: std::sync::OnceLock<CompiledProgram> = std::sync::OnceLock::new();
    PROGRAM.get_or_init(|| build_program(wfgrep_module()))
}

/// The trusted reference search, over a real directory tree.
///
/// This is the frozen contract wfgrep implements: entries of one directory
/// are visited in ascending name-byte order with a shorter prefix first, a
/// directory is descended as soon as it is reached, a line is a maximal run
/// of bytes between newlines, an unterminated final run is a line, the
/// pattern is matched against the line without its terminator, the empty
/// pattern matches every line, and every published record is
/// `PATH:LINE:TEXT` with the line's own terminator.
fn reference(root: &Path, display: &[u8], pattern: &[u8]) -> (Vec<u8>, i32) {
    let mut published = Vec::new();
    let mut matched = false;
    visit(root, display, pattern, &mut published, &mut matched);
    (published, i32::from(!matched))
}

fn visit(
    directory: &Path,
    display: &[u8],
    pattern: &[u8],
    published: &mut Vec<u8>,
    hit: &mut bool,
) {
    // A DirEntry may retain its directory handle. Store only value metadata
    // before descending, so the reference does not consume a handle per level.
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .expect("read the reference fixture directory")
        .map(|entry| {
            let entry = entry.expect("one reference fixture entry");
            (
                entry.file_name(),
                entry.path(),
                entry.file_type().expect("one reference fixture entry kind"),
            )
        })
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, entry_path, kind) in entries {
        let name = name.as_bytes().to_vec();
        let mut path = display.to_vec();
        if !path.is_empty() {
            path.push(b'/');
        }
        path.extend_from_slice(&name);
        if kind.is_file() {
            let content = std::fs::read(&entry_path).expect("read one reference fixture file");
            for (ordinal, line) in lines(&content).into_iter().enumerate() {
                if occurs(line, pattern) {
                    *hit = true;
                    published.extend_from_slice(&path);
                    published.push(b':');
                    published.extend_from_slice((ordinal + 1).to_string().as_bytes());
                    published.push(b':');
                    published.extend_from_slice(line);
                    published.push(b'\n');
                }
            }
        } else if kind.is_dir() {
            visit(&entry_path, &path, pattern, published, hit);
        }
    }
}

fn lines(content: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, byte) in content.iter().enumerate() {
        if *byte == b'\n' {
            lines.push(&content[start..index]);
            start = index + 1;
        }
    }
    if start < content.len() {
        lines.push(&content[start..]);
    }
    lines
}

fn occurs(line: &[u8], pattern: &[u8]) -> bool {
    pattern.is_empty()
        || line
            .windows(pattern.len().max(1))
            .any(|window| window == pattern)
}

/// Runs wfgrep over one fixture tree and compares it with the reference.
fn assert_reference(program: &CompiledProgram, root: &Path, tree: &str, pattern: &[u8]) -> Vec<u8> {
    let output = program.run(root, &[pattern, tree.as_bytes()]);
    let (expected, status) = reference(&root.join(tree), tree.as_bytes(), pattern);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&expected),
        "published bytes disagree with the reference search"
    );
    assert_eq!(
        output.status.code(),
        Some(status),
        "status disagrees with the reference search; diagnostics: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// The host `grep -rn` over the same tree, as an independent hit set.
///
/// Both sides are sorted because `grep -r` visits the tree in the host's own
/// enumeration order while wfgrep visits it in sorted order; the hit *set* is
/// the property under test, not the order the two tools chose.
fn grep_rn(root: &Path, tree: &str, pattern: &[u8]) -> Vec<String> {
    let output = Command::new("/usr/bin/grep")
        .arg("-rn")
        .arg("-e")
        .arg(std::ffi::OsStr::from_bytes(pattern))
        .arg(tree)
        .current_dir(root)
        .output()
        .expect("invoke the host grep");
    sorted_lines(&output.stdout)
}

fn sorted_lines(bytes: &[u8]) -> Vec<String> {
    let mut lines: Vec<String> = String::from_utf8_lossy(bytes)
        .lines()
        .map(str::to_owned)
        .collect();
    lines.sort();
    lines
}

/// One fixture tree both oracles read, deliberately built out of the shapes
/// the two grep families agree on: every file is terminated text, every name
/// is ordinary, and no symbolic link is involved.
fn search_tree() -> super::support::FixtureDirectory {
    let fixture = fixture_directory();
    fixture.directory("tree");
    fixture.write_nested("tree/alpha.txt", b"needle here\nplain\nneedle again\n");
    fixture.write_nested("tree/beta.txt", b"nothing\nat all\n");
    fixture.write_nested("tree/.hidden.txt", b"hidden needle\n");
    fixture.write_nested("tree/sub/gamma.txt", b"deep needle\n");
    fixture.write_nested("tree/sub/delta.txt", b"no match here\n");
    fixture.write_nested("tree/sub/deeper/epsilon.txt", b"deepest needle\nlast\n");
    fixture.write_nested("tree/zeta.txt", b"needle at the end\n");
    fixture
}

/// The headline evidence: a real recursive search over a real tree, checked
/// against the reference contract and against the host's own `grep -rn`.
#[test]
fn wfgrep_searches_a_real_tree_and_agrees_with_grep() {
    let fixture = search_tree();
    let published = assert_reference(wfgrep(), fixture.path(), "tree", b"needle");
    assert_eq!(
        sorted_lines(&published),
        grep_rn(fixture.path(), "tree", b"needle"),
        "wfgrep and grep -rn disagree about the hit set"
    );
}

/// wfgrep linked from the link fragments a modular build splits it into
/// [MOD-8], in both granularities, publishes what the reference search
/// publishes: its constants and runtime helpers each keep one definition,
/// named across the fragment boundaries.
#[test]
fn wfgrep_linked_from_its_fragments_searches_as_the_whole_module() {
    let fixture = search_tree();
    for granularity in [FragmentGranularity::Function, FragmentGranularity::Module] {
        let program = build_program_from_fragments(wfgrep_module(), granularity);
        assert_reference(&program, fixture.path(), "tree", b"needle");
        let absent = program.run(fixture.path(), &[b"absent-pattern", b"tree"]);
        assert_eq!(absent.status.code(), Some(1), "{granularity:?}");
        assert!(absent.stdout.is_empty(), "{granularity:?}");
    }
}

/// The same cross-check with a pattern that matches nothing, so the empty
/// hit set is a real agreement rather than a vacuous one, and with a pattern
/// that matches every line, so the full hit set is too.
#[test]
fn wfgrep_agrees_with_grep_on_the_empty_and_the_total_hit_set() {
    let fixture = search_tree();
    let absent = wfgrep().run(fixture.path(), &[b"absent-pattern", b"tree"]);
    assert_eq!(absent.status.code(), Some(1));
    assert!(absent.stdout.is_empty());
    assert_eq!(
        grep_rn(fixture.path(), "tree", b"absent-pattern"),
        Vec::<String>::new()
    );

    let published = assert_reference(wfgrep(), fixture.path(), "tree", b"e");
    assert_eq!(
        sorted_lines(&published),
        grep_rn(fixture.path(), "tree", b"e"),
        "wfgrep and grep -rn disagree about the total hit set"
    );
}

/// The former sixteen-level cutoff silently skipped a matching file while
/// reporting a normal result. Seventeen descents distinguish that defect;
/// a shallow hit also checks that the result is not merely an error/empty walk.
/// Arbitrary deeper stress is not another correctness boundary: the program
/// retains a host directory handle per level, so it also requires a declared
/// host descriptor budget. Runtime stack/path limits have their own tests.
#[test]
fn a_tree_beyond_the_former_sixteen_level_cap_is_searched_completely() {
    let fixture = fixture_directory();
    let mut relative = String::from("tree");
    for _ in 0..17 {
        relative.push_str("/d");
    }
    relative.push_str("/bottom.txt");
    fixture.write_nested(&relative, b"needle at the bottom\n");
    fixture.write_nested("tree/top.txt", b"needle at the top\n");

    let published = assert_reference(wfgrep(), fixture.path(), "tree", b"needle");
    assert_eq!(
        sorted_lines(&published).len(),
        2,
        "the deep file and the shallow one are both hits"
    );
}

/// A search root written with several components is searched.
///
/// `open_directory` opens exactly one component and refuses a separator with
/// `InvalidPath`, so the program once handed it `top/middle/tree` whole,
/// reported every such root as `cannot read`, and searched nothing. The root
/// is now opened one component at a time. Trailing separators leave the
/// display prefix unchanged, as `grep -r` prints it; a regular file named
/// below several directories is still searched as a single file; and a
/// missing middle component is reported by its own class.
#[test]
fn a_root_with_several_components_is_searched() {
    let fixture = fixture_directory();
    fixture.write_nested("top/middle/tree/alpha.txt", b"needle here\nplain\n");
    fixture.write_nested("top/middle/tree/sub/beta.txt", b"deep needle\n");
    fixture.write_nested("top/middle/outside.txt", b"needle outside the root\n");

    let published = assert_reference(wfgrep(), fixture.path(), "top/middle/tree", b"needle");
    assert_eq!(
        sorted_lines(&published),
        grep_rn(fixture.path(), "top/middle/tree", b"needle"),
        "wfgrep and grep -rn disagree about a root with several components"
    );

    for spelling in ["top/middle/tree/", "top//middle/tree//"] {
        let output = wfgrep().run(fixture.path(), &[b"needle", spelling.as_bytes()]);
        let shown = if spelling.starts_with("top//") {
            String::from_utf8_lossy(&published).replace("top/middle", "top//middle")
        } else {
            String::from_utf8_lossy(&published).into_owned()
        };
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            shown,
            "the root spelled {spelling:?} displays differently"
        );
        assert_eq!(output.status.code(), Some(0));
    }

    let single = wfgrep().run(
        fixture.path(),
        &[b"needle", b"top/middle/tree/sub/beta.txt"],
    );
    assert_eq!(
        String::from_utf8_lossy(&single.stdout),
        "top/middle/tree/sub/beta.txt:1:deep needle\n"
    );
    assert_eq!(single.status.code(), Some(0));

    let missing = wfgrep().run(fixture.path(), &[b"needle", b"top/absent/tree"]);
    assert!(missing.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&missing.stderr),
        "wfgrep: top/absent/tree: no such file or directory\n"
    );
    assert_eq!(missing.status.code(), Some(2));
}

/// A directory with far more entries than the former cap, and more name
/// bytes than one enumeration batch holds, is searched completely.
///
/// The walk once kept the first 64 entries in the host's enumeration order
/// and dropped the rest while returning normally, so a truncated search was
/// indistinguishable from a complete one; this repository's `spec` directory
/// lost six files that way. Long names spread the top directory over several
/// 8192-byte enumeration batches, the subdirectories repeat the shape one
/// level down, and the entry that sorts last carries a match.
#[test]
fn a_directory_past_the_former_entry_cap_is_searched_completely() {
    let fixture = fixture_directory();
    fixture.directory("tree");
    let long = "n".repeat(200);
    for index in 0..300 {
        let content: &[u8] = if index % 50 == 49 {
            b"needle in a long name\n"
        } else {
            b"plain\n"
        };
        fixture.write_nested(&format!("tree/{long}-{index:03}.txt"), content);
    }
    for directory in 0..8 {
        for index in 0..(FORMER_ENTRY_CAP + 16) {
            let content: &[u8] = if index == FORMER_ENTRY_CAP + 15 {
                b"needle one level down\n"
            } else {
                b"plain\n"
            };
            fixture.write_nested(
                &format!("tree/sub-{directory}/file-{index:03}.txt"),
                content,
            );
        }
    }
    fixture.write_nested("tree/zzzz-last.txt", b"needle sorted last\n");

    let published = assert_reference(wfgrep(), fixture.path(), "tree", b"needle");
    let hits = sorted_lines(&published);
    assert_eq!(hits.len(), 6 + 8 + 1, "every hit is published");
    assert!(hits.contains(&"tree/zzzz-last.txt:1:needle sorted last".to_owned()));
    assert_eq!(
        hits,
        grep_rn(fixture.path(), "tree", b"needle"),
        "wfgrep and grep -rn disagree about a large directory"
    );
}

/// Lines longer than the initial read window, and records longer than the
/// output batch, are searched and published whole, as a real grep does.
///
/// The read window was fixed at 4096 bytes and a longer line stopped its file
/// with `line too long`, which left 21 of this repository's specification
/// archives unsearched past their first long line. The window now doubles
/// while one incomplete line fills it. The cases cover a line exactly one
/// window long, a match straddling the first window boundary, a record three
/// batches long with the match at its end, an unmatched long line between
/// matches, and an unterminated final line longer than the window.
#[test]
fn lines_longer_than_the_read_window_are_searched_and_published_whole() {
    let fixture = fixture_directory();
    fixture.directory("tree");
    let mut content = b"short needle line\n".to_vec();
    let mut exact = vec![b'x'; BUFFER_LENGTH - 6];
    exact.extend_from_slice(b"needle");
    content.extend_from_slice(&exact);
    content.push(b'\n');
    let mut straddling = vec![b'a'; BUFFER_LENGTH - 3];
    straddling.extend_from_slice(b"needle");
    straddling.extend_from_slice(&[b'b'; 100]);
    content.extend_from_slice(&straddling);
    content.push(b'\n');
    let mut wide = vec![b'c'; BATCH_LENGTH * 3];
    wide.extend_from_slice(b"needle");
    content.extend_from_slice(&wide);
    content.push(b'\n');
    content.extend_from_slice(&vec![b'd'; BUFFER_LENGTH * 5]);
    content.push(b'\n');
    content.extend_from_slice(b"needle after the long lines\n");
    let mut last = b"needle ".to_vec();
    last.extend_from_slice(&vec![b'e'; BUFFER_LENGTH * 2 + 1]);
    content.extend_from_slice(&last);
    fixture.write_nested("tree/long.txt", &content);
    fixture.write_nested("tree/short.txt", b"needle in a short file\n");

    let published = assert_reference(wfgrep(), fixture.path(), "tree", b"needle");
    assert_eq!(
        sorted_lines(&published).len(),
        7,
        "every matching line, long or short, is published"
    );
}

/// A refused write to standard output stops the search and is reported once,
/// against standard output rather than against a searched path.
///
/// The broken pipe was once classified as an unnamed read status, so every
/// later file with a match was reported as `PATH: cannot read` while the walk
/// read the rest of the tree for a reader that had already gone. The fixture
/// holds several matching files, so a search that continued past the first
/// refusal would publish more than one diagnostic.
#[test]
fn a_closed_standard_output_stops_the_search_without_blaming_an_input() {
    let fixture = search_tree();
    let (status, diagnostics) =
        wfgrep().run_with_closed_output(fixture.path(), &[b"needle", b"tree"]);
    assert_eq!(
        status.signal(),
        None,
        "a write to a closed destination must not kill the process"
    );
    assert_eq!(
        String::from_utf8_lossy(&diagnostics),
        "wfgrep: standard output: broken pipe\n"
    );
    assert_eq!(status.code(), Some(2));
}

/// A match that straddles a read boundary is still one match, and the line
/// number a straddling line receives is still its ordinal in the file.
#[test]
fn a_match_across_a_read_boundary_keeps_its_line_number() {
    let fixture = fixture_directory();
    fixture.directory("tree");
    let mut content = Vec::new();
    for ordinal in 0..8 {
        content.extend_from_slice(format!("line {ordinal}\n").as_bytes());
    }
    while content.len() < BUFFER_LENGTH - 4 {
        content.extend_from_slice(b"filler line\n");
    }
    content.extend_from_slice(b"xx needle straddles here\n");
    content.extend_from_slice(b"tail\n");
    fixture.write_nested("tree/wide.txt", &content);

    let published = assert_reference(wfgrep(), fixture.path(), "tree", b"needle");
    assert_eq!(
        sorted_lines(&published),
        grep_rn(fixture.path(), "tree", b"needle")
    );
}

/// An empty tree publishes nothing and reports no match, which is the
/// no-match status rather than an error.
#[test]
fn an_empty_tree_publishes_nothing_and_reports_no_match() {
    let fixture = fixture_directory();
    fixture.directory("tree");
    let output = wfgrep().run(fixture.path(), &[b"needle", b"tree"]);
    assert!(output.stdout.is_empty());
    assert_eq!(output.status.code(), Some(1));
}

/// A subdirectory the process cannot open is an ordinary recoverable outcome:
/// the search reports it on standard error, keeps searching everything else,
/// and ends with the error status.
#[test]
fn an_unreadable_subdirectory_is_reported_and_the_rest_is_still_searched() {
    let fixture = fixture_directory();
    fixture.directory("tree");
    fixture.write_nested("tree/open.txt", b"needle visible\n");
    let closed = fixture.directory("tree/closed");
    fixture.write_nested("tree/closed/buried.txt", b"needle buried\n");
    close_path(&closed);

    let output = wfgrep().run(fixture.path(), &[b"needle", b"tree"]);

    reopen_path(&closed, 0o755);

    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "tree/open.txt:1:needle visible\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "wfgrep: tree/closed: permission denied\n"
    );
    assert_eq!(output.status.code(), Some(2));
}

/// A file the process cannot open is reported by its complete relative path
/// and does not stop the walk.
#[test]
fn an_unreadable_file_is_reported_by_path_and_the_walk_continues() {
    let fixture = fixture_directory();
    fixture.directory("tree");
    let denied = fixture.write_nested("tree/denied.txt", b"needle denied\n");
    fixture.write_nested("tree/open.txt", b"needle visible\n");
    close_path(&denied);

    let output = wfgrep().run(fixture.path(), &[b"needle", b"tree"]);

    reopen_path(&denied, 0o644);

    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "tree/open.txt:1:needle visible\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "wfgrep: tree/denied.txt: permission denied\n"
    );
    assert_eq!(output.status.code(), Some(2));
}

/// A search root the capability cannot open is reported once and is the
/// error status, not an empty successful search.
#[test]
fn a_missing_search_root_is_reported_once() {
    let fixture = fixture_directory();
    let output = wfgrep().run(fixture.path(), &[b"needle", b"absent"]);
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "wfgrep: absent: no such file or directory\n"
    );
    assert_eq!(output.status.code(), Some(2));
}

/// An invocation naming no root reports its usage and the error status.
#[test]
fn wfgrep_reports_its_usage_when_the_invocation_names_no_root() {
    let fixture = fixture_directory();
    let output = wfgrep().run(fixture.path(), &[b"needle"]);
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "usage: wfgrep PATTERN ROOT\n"
    );
    assert_eq!(output.status.code(), Some(2));
}

/// A pattern that is not valid text travels the lossless route unchanged and
/// matches the same bytes in the file: nothing on the route from the
/// invocation argument to the comparison passes through text.
///
/// The host is not asked to hold a file *name* that is not text — the
/// supported hosts' filesystems refuse one — so the non-text bytes travel
/// through the argument and the file content, which is where [HOST-2]'s
/// lossless route actually runs. This case has no `grep` side: the two grep
/// families disagree about a pattern that is not text.
#[test]
fn a_pattern_that_is_not_text_travels_the_lossless_route_unchanged() {
    let fixture = fixture_directory();
    fixture.directory("tree");
    fixture.write_nested(
        "tree/raw.txt",
        b"prefix \xff\xfe marker suffix\nplain line\n",
    );

    let output = wfgrep().run(fixture.path(), &[b"\xff\xfe marker", b"tree"]);
    assert_eq!(
        output.stdout,
        b"tree/raw.txt:1:prefix \xff\xfe marker suffix\n".to_vec(),
        "diagnostics: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(0));
}

/// A symbolic link the walk enumerates is not followed.
///
/// The ordinary directory entry reports kind `3 symbolic link`; the program acts on
/// exactly the kinds it was told about — a regular file it opens, a directory
/// it descends, and everything else it leaves alone. This checks that the
/// program skips the link before attempting a component open. The case has
/// no `grep` side, because the two grep families disagree about
/// links found during a traversal.
#[test]
fn an_enumerated_symbolic_link_is_not_followed() {
    let fixture = fixture_directory();
    fixture.directory("tree");
    fixture.write_nested("tree/visible.txt", b"needle inside\n");
    let outside = fixture.write(b"outside.txt", b"needle outside\n");
    let outside_directory = fixture.directory("elsewhere");
    fixture.write_nested("elsewhere/buried.txt", b"needle elsewhere\n");
    fixture.symlink("tree/link.txt", &outside);
    fixture.symlink("tree/linkdir", &outside_directory);

    let output = wfgrep().run(fixture.path(), &[b"needle", b"tree"]);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "tree/visible.txt:1:needle inside\n"
    );
    assert_eq!(output.status.code(), Some(0));
}

/// The search reaches its file and directory operations through ordinary
/// direct calls. A supplied declaration alone does not establish a call site.
#[test]
fn the_search_uses_ordinary_file_and_directory_calls() {
    let llvm = wfgrep_module();
    for name in [
        "open_file",
        "open_directory_source",
        "directory_next",
        "open_directory",
        "read_at",
    ] {
        assert!(
            llvm.contains(&format!("call void @wf_std.fs.{name}(")),
            "the search must call the ordinary {name} declaration"
        );
    }
}

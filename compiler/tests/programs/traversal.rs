//! Ordinary directory values and std::fs functions, end to end.
//!
//! Every case here compiles a real corpus program against the declared
//! inventory, links it, and runs it against a real directory tree the harness
//! writes with ordinary filesystem calls. Nothing is injected into the
//! program's address space: it opens, enumerates, and descends through the
//! host's own facilities, exactly as an ordinary linked program would.

use std::process::Output;

use super::support::{
    CompiledProgram, build_program, close_path, compile_program, fixture_directory, reopen_path,
};

/// The traversal program itself: a recursive walk of the invocation
/// directory that records every entry's kind and relative path in one fixed
/// store of 64 records of 128 bytes, and publishes the recorded entries in
/// sorted order.
///
/// A green run on the six-entry fixture, a tree three levels deep, establishes
/// that `open_directory_source` produced an independent enumeration handle,
/// that `directory_next` normalized the host's own records into the portable
/// form, and that `open_directory` opened each child ordinary directory value
/// by name bytes with no path value ever formed.
///
/// The store is fixed, so the program has two limits, a directory holding more
/// than 64 entries in all and a relative path longer than 126 bytes, and it
/// reports each one on standard error and exits 3 rather than succeeding
/// with a listing that lacks entries. The descent has no depth limit of its
/// own: the path limit is the only bound on how deep a walk goes. The cases
/// after the fixture build trees at and just past each limit, and every one of
/// them is collected before the test asserts, so one run names every case that
/// failed.
#[test]
fn the_traversal_program_walks_a_real_tree_and_publishes_it_sorted() {
    let llvm = compile_program("dir_walk.wf");
    // Compilation itself establishes every source-level array and buffer
    // domain. The emitted module may still carry the separately deferred heap
    // resource-abort path, but no source proof failure may survive lowering.
    assert!(
        !llvm.contains("call void @wf_trap(ptr @.wf_trap."),
        "the traversal must execute only operations admitted by static proof"
    );
    // C2 deletes QUAL-1 and the compiler-owned native wrapper. All three
    // operations use the ordinary callable ABI; the native engine is linked
    // separately and contributes no compiler declaration or permission.
    // Each is a waiting host function, linked as its start and its finish.
    assert!(llvm.contains("@wf_std.fs.open_directory_source.start("));
    assert!(llvm.contains("@wf_std.fs.directory_next.start("));
    assert!(llvm.contains("@wf_std.fs.open_directory.start("));
    assert!(!llvm.contains("@wf__completion_directory_next_submit("));

    let program = build_program(&llvm);
    let fixture = fixture_directory();
    fixture.write(b"a.txt", b"first\n");
    fixture.write(b"z.txt", b"last\n");
    fixture.write_nested("sub/b.txt", b"second\n");
    fixture.write_nested("sub/deeper/c.txt", b"third\n");

    let output = program.run(fixture.path(), &[]);
    assert!(
        output.status.success(),
        "traversal exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "1 a.txt\n2 sub\n1 sub/b.txt\n2 sub/deeper\n1 sub/deeper/c.txt\n1 z.txt\n"
    );
    an_empty_tree_publishes_nothing(&program);
    an_unreadable_subdirectory_is_recorded_without_descending_into_it(&program);

    let cases: [(&str, Result<(), String>); 7] = [
        (
            "more_than_sixty_four_entries_are_reported",
            more_than_sixty_four_entries_are_reported(&program),
        ),
        (
            "exactly_sixty_four_entries_are_published_whole",
            exactly_sixty_four_entries_are_published_whole(&program),
        ),
        (
            "a_tree_deeper_than_eight_levels_is_published_whole",
            a_tree_deeper_than_eight_levels_is_published_whole(&program),
        ),
        (
            "a_path_of_127_bytes_is_reported",
            a_path_of_127_bytes_is_reported(&program),
        ),
        (
            "a_path_of_126_bytes_is_published_whole",
            a_path_of_126_bytes_is_published_whole(&program),
        ),
        (
            "a_tree_deeper_than_the_path_allows_is_reported",
            a_tree_deeper_than_the_path_allows_is_reported(&program),
        ),
        (
            "a_tree_as_deep_as_the_path_allows_is_published_whole",
            a_tree_as_deep_as_the_path_allows_is_published_whole(&program),
        ),
    ];
    let failures: Vec<String> = cases
        .into_iter()
        .filter_map(|(name, result)| result.err().map(|why| format!("{name}: {why}")))
        .collect();
    assert!(
        failures.is_empty(),
        "limit cases failed:\n{}",
        failures.join("\n")
    );
}

/// The standard error the program gives for a listing of more than 64 entries.
const ENTRY_LIMIT_MESSAGE: &str = "more than 64 entries";

/// The standard error the program gives for a path past 126 bytes.
const PATH_LIMIT_MESSAGE: &str = "126 bytes";

/// What a run did, in one line for a failure message.
fn describe(output: &Output) -> String {
    format!(
        "exit {:?}, stderr {:?}, stdout {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    )
}

/// A run that ended cleanly with nothing on standard error, whose standard
/// output is exactly `expected`.
fn expect_published_whole(output: &Output, expected: &str) -> Result<(), String> {
    if output.status.code() != Some(0) {
        return Err(format!("expected exit 0, got {}", describe(output)));
    }
    if !output.stderr.is_empty() {
        return Err(format!("expected empty stderr, got {}", describe(output)));
    }
    let published = String::from_utf8_lossy(&output.stdout);
    if published != expected {
        return Err(format!(
            "expected stdout {expected:?}, got {}",
            describe(output)
        ));
    }
    Ok(())
}

/// A run that exited 3 and named `message` on standard error.
fn expect_reported(output: &Output, message: &str) -> Result<(), String> {
    if output.status.code() != Some(3) {
        return Err(format!("expected exit 3, got {}", describe(output)));
    }
    if !String::from_utf8_lossy(&output.stderr).contains(message) {
        return Err(format!(
            "expected stderr naming {message:?}, got {}",
            describe(output)
        ));
    }
    Ok(())
}

/// `count` one-byte directory names joined by `/`.
fn chain(count: usize) -> String {
    vec!["d"; count].join("/")
}

/// What a walk of a chain of `levels` directories holding one file `x` at the
/// bottom publishes: each directory, outermost first, then the file.
fn chain_listing(levels: usize) -> String {
    let mut expected = String::new();
    for level in 1..=levels {
        expected.push_str(&format!("2 {}\n", chain(level)));
    }
    expected.push_str(&format!("1 {}/x\n", chain(levels)));
    expected
}

/// A hundred files cannot all be recorded: the program publishes the 64 it
/// recorded, sorted, and then reports the limit and exits 3.
fn more_than_sixty_four_entries_are_reported(program: &CompiledProgram) -> Result<(), String> {
    let fixture = fixture_directory();
    let names: Vec<String> = (0..100).map(|number| format!("f{number:03}")).collect();
    for name in &names {
        fixture.write_nested(name, b"x\n");
    }
    let output = program.run(fixture.path(), &[]);
    expect_reported(&output, ENTRY_LIMIT_MESSAGE)?;
    let published = String::from_utf8_lossy(&output.stdout).into_owned();
    let lines: Vec<&str> = published.lines().collect();
    if lines.len() != 64 {
        return Err(format!(
            "expected 64 published lines, got {}",
            describe(&output)
        ));
    }
    let mut previous: Option<&str> = None;
    for line in &lines {
        let name = line
            .strip_prefix("1 ")
            .filter(|name| names.iter().any(|written| written == name))
            .ok_or_else(|| format!("line {line:?} is no written file: {}", describe(&output)))?;
        if previous.is_some_and(|before| before >= name) {
            return Err(format!(
                "lines are not strictly ascending: {}",
                describe(&output)
            ));
        }
        previous = Some(name);
    }
    Ok(())
}

/// Exactly 64 entries fill the store without reaching its limit.
fn exactly_sixty_four_entries_are_published_whole(
    program: &CompiledProgram,
) -> Result<(), String> {
    let fixture = fixture_directory();
    let mut expected = String::new();
    for number in 0..64 {
        let name = format!("f{number:02}");
        fixture.write_nested(&name, b"x\n");
        expected.push_str(&format!("1 {name}\n"));
    }
    expect_published_whole(&program.run(fixture.path(), &[]), &expected)
}

/// A tree of twelve directories and a file, more levels than the program once
/// stopped at, is published whole.
fn a_tree_deeper_than_eight_levels_is_published_whole(
    program: &CompiledProgram,
) -> Result<(), String> {
    let fixture = fixture_directory();
    fixture.write_nested(&format!("{}/x", chain(12)), b"x\n");
    expect_published_whole(&program.run(fixture.path(), &[]), &chain_listing(12))
}

/// A directory name of 100 bytes with a file name of 26 bytes below it makes a
/// relative path of 127 bytes: the directory is published, the file is not, and
/// the limit is reported.
fn a_path_of_127_bytes_is_reported(program: &CompiledProgram) -> Result<(), String> {
    let fixture = fixture_directory();
    let directory = "a".repeat(100);
    fixture.write_nested(&format!("{directory}/{}", "b".repeat(26)), b"x\n");
    let output = program.run(fixture.path(), &[]);
    expect_reported(&output, PATH_LIMIT_MESSAGE)?;
    let published = String::from_utf8_lossy(&output.stdout);
    if published != format!("2 {directory}\n") {
        return Err(format!(
            "expected only the directory published, got {}",
            describe(&output)
        ));
    }
    Ok(())
}

/// The same tree with a 25-byte file name makes a path of exactly 126 bytes,
/// which the program records whole.
fn a_path_of_126_bytes_is_published_whole(program: &CompiledProgram) -> Result<(), String> {
    let fixture = fixture_directory();
    let directory = "a".repeat(100);
    let file = "b".repeat(25);
    fixture.write_nested(&format!("{directory}/{file}"), b"x\n");
    expect_published_whole(
        &program.run(fixture.path(), &[]),
        &format!("2 {directory}\n1 {directory}/{file}\n"),
    )
}

/// Sixty-three one-byte directories and a file make a path of 127 bytes: the
/// descent ends at the path limit, which is reported.
fn a_tree_deeper_than_the_path_allows_is_reported(
    program: &CompiledProgram,
) -> Result<(), String> {
    let fixture = fixture_directory();
    fixture.write_nested(&format!("{}/x", chain(63)), b"x\n");
    let output = program.run(fixture.path(), &[]);
    expect_reported(&output, PATH_LIMIT_MESSAGE)?;
    let directories: String = (1..=63)
        .map(|level| format!("2 {}\n", chain(level)))
        .collect();
    let published = String::from_utf8_lossy(&output.stdout);
    if published != directories {
        return Err(format!(
            "expected the 63 directories only, got {}",
            describe(&output)
        ));
    }
    Ok(())
}

/// Sixty-two one-byte directories and a file make a path of 125 bytes, the
/// deepest tree the path limit lets through, and it is published whole.
fn a_tree_as_deep_as_the_path_allows_is_published_whole(
    program: &CompiledProgram,
) -> Result<(), String> {
    let fixture = fixture_directory();
    fixture.write_nested(&format!("{}/x", chain(62)), b"x\n");
    expect_published_whole(&program.run(fixture.path(), &[]), &chain_listing(62))
}

/// An empty directory yields no entry: the listing leaves out the self and
/// parent entries [PRE-2], and the program, which no longer skips them, would
/// otherwise descend into them.
fn an_empty_tree_publishes_nothing(
    program: &CompiledProgram,
) {
    let fixture = fixture_directory();
    let output = program.run(fixture.path(), &[]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

/// A name the program cannot open still ends the walk cleanly: the entry is
/// recorded from its enumeration record and the failed descent is an ordinary
/// recoverable outcome rather than a failed source proof.
fn an_unreadable_subdirectory_is_recorded_without_descending_into_it(program: &CompiledProgram) {
    let fixture = fixture_directory();
    fixture.write(b"a.txt", b"first\n");
    let closed = fixture.directory("closed");
    fixture.write_nested("closed/hidden.txt", b"hidden\n");
    close_path(&closed);

    let output = program.run(fixture.path(), &[]);

    reopen_path(&closed, 0o755);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "1 a.txt\n2 closed\n"
    );
}

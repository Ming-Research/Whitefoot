//! The runtime's own frames over one page, read from assembly.
//!
//! Generated code carries a stack-probe attribute on every definition, so a
//! frame larger than the guard page touches each page on its way down and a
//! stack exhaustion ends in the floor's `{"resource":"stack"}` record. The
//! runtime's units are C and hand-written LLVM that clang compiles with the
//! driver's own arguments, and upstream clang adds no probe to C by default.
//! This reads, for every unit any link compiles, each frame over a page or of
//! non-fixed size from `-fstack-usage` and demands the probe in its assembly,
//! so a frame the runtime grows later is caught on the host this runs on
//! (design decision `runtime-c-units-carry-the-generated-probe-method`).

use std::path::Path;

use super::{
    HOST_OPTIMIZATION_ARGUMENTS, TARGET_COMPILE_ARGUMENTS, resolved_toolchain,
    restricted_child_arguments, runtime_units, unit_arguments,
};

/// The guard page's size, the largest fixed frame that needs no probe.
const PAGE: u64 = 4096;

/// One row of clang's `-fstack-usage` report: `file:line:column:name`, the
/// frame's bytes, and `static`, `dynamic` or `dynamic,bounded`.
#[derive(Debug, Eq, PartialEq)]
struct Frame {
    name: String,
    bytes: u64,
    kind: String,
}

impl Frame {
    /// A frame over a page, or whose size is not fixed, must be probed.
    fn needs_probe(&self) -> bool {
        self.bytes > PAGE || self.kind != "static"
    }
}

fn frames(report: &str) -> Vec<Frame> {
    report
        .lines()
        .filter_map(|row| {
            let fields: Vec<&str> = row.split('\t').collect();
            let [location, bytes, kind] = fields[..] else {
                return None;
            };
            Some(Frame {
                name: location.rsplit(':').next()?.to_owned(),
                bytes: bytes.parse().ok()?,
                kind: kind.to_owned(),
            })
        })
        .collect()
}

/// The assembly lines of one definition, from its label to its end. clang
/// writes the label with a comment after it (`name:  # @name`, and
/// `_name:  ## @name` on Darwin), so the label is the line's first word.
fn body<'a>(assembly: &'a str, name: &str) -> Vec<&'a str> {
    let labels = [format!("{name}:"), format!("_{name}:")];
    assembly
        .lines()
        .skip_while(|line| {
            !line
                .split_whitespace()
                .next()
                .is_some_and(|word| labels.iter().any(|label| label == word))
        })
        .take_while(|line| !line.trim_start().starts_with(".cfi_endproc"))
        .collect()
}

/// Whether a body reaches its frame page by page, or calls Apple's helper.
///
/// An inline probe moves the stack pointer one page and stores into it, in a
/// loop or repeated, so it holds both halves. A store of zero at the stack
/// pointer alone is not one: it also zeroes an ordinary local, and a single
/// move of the whole frame is the unprobed step.
fn probes(body: &[&str]) -> bool {
    let lines: Vec<String> = body
        .iter()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    let has = |text: &str| lines.iter().any(|line| line.contains(text));
    has("___chkstk_darwin")
        || (has("subq $4096, %rsp") && has("movq $0, (%rsp)"))
        || (has("sub sp, sp, #1, lsl #12") && has("str xzr, [sp]"))
}

/// The clang this check compiles with: the toolchain the driver resolves.
fn compiler_line() -> String {
    let toolchain = resolved_toolchain().expect("the toolchain resolves");
    #[allow(clippy::disallowed_methods, reason = "host toolchain, not a test program")]
    let output = toolchain
        .command()
        .arg("--version")
        .output()
        .expect("run clang --version");
    let version = String::from_utf8_lossy(&output.stdout);
    format!(
        "runtime frames compiled by {}: {}",
        toolchain.clang().display(),
        version.lines().next().unwrap_or_default()
    )
}

/// Compile one runtime unit as the driver does and return its assembly and
/// its stack-usage report (empty where clang opened none).
fn compile(directory: &Path, relative_path: &str, probe: &[&str]) -> (String, String) {
    // One output per unit file name: `ordinary_values.c` and
    // `ordinary_values.ll` share a stem, and a unit that leaves no report
    // must not be read through the previous unit's.
    let assembly = directory.join(format!("{relative_path}.s"));
    let mut command = resolved_toolchain().expect("the toolchain resolves").command();
    if relative_path.ends_with(".ll") {
        command
            .args(TARGET_COMPILE_ARGUMENTS)
            .arg("-Wno-override-module")
            .args(["-x", "ir"]);
    } else {
        unit_arguments(&mut command, directory, None, probe);
        command.args(restricted_child_arguments(relative_path));
    }
    #[allow(clippy::disallowed_methods, reason = "host toolchain, not a test program")]
    let output = command
        .args(HOST_OPTIMIZATION_ARGUMENTS)
        .args(["-S", "-fstack-usage", "-o"])
        .arg(&assembly)
        .arg(directory.join(relative_path))
        .output()
        .expect("start the host compiler");
    assert!(
        output.status.success(),
        "{relative_path}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = std::fs::read_to_string(&assembly).expect("read the assembly");
    // clang opens the report at the first function it emits, so a unit that
    // defines none on this host leaves none.
    let usage = std::fs::read_to_string(assembly.with_extension("su")).unwrap_or_default();
    (text, usage)
}

/// Every unit a link compiles, with the allocator unit a heap reference adds.
/// A frame over a page in any of them must reach its probe. The corpus group
/// of the gate runs it, with the clang the host resolves.
///
/// The hand-written LLVM unit is read as IR, where no compile argument adds a
/// probe: its definitions carry no attributes, so a frame over a page there
/// fails this test until the definition's text gains a `probe-stack`
/// attribute.
#[test]
fn every_runtime_c_frame_over_a_page_is_probed() {
    println!("{}", compiler_line());
    let probe = whitefoot::runtime_stack_probe_arguments().expect("the probe arguments resolve");
    let (staged, compiled) = runtime_units("@wf__heap_take(");
    let directory = std::env::temp_dir().join(format!(
        "whitefootc-runtime-frames-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    for unit in &staged {
        let path = directory.join(unit.relative_path);
        std::fs::create_dir_all(path.parent().expect("a staged unit has a directory"))
            .expect("stage a runtime directory");
        std::fs::write(&path, unit.source).expect("stage a runtime unit");
    }
    let mut unprobed = Vec::new();
    let mut large = Vec::new();
    for relative_path in compiled
        .iter()
        .filter(|name| name.ends_with(".c") || name.ends_with(".ll"))
    {
        let (assembly, usage) = compile(&directory, relative_path, probe);
        let rows = frames(&usage);
        assert!(
            !rows.is_empty() || !assembly.contains(".cfi_startproc"),
            "{relative_path}: functions but no readable stack-usage report"
        );
        for frame in rows.iter().filter(|frame| frame.needs_probe()) {
            let lines = body(&assembly, &frame.name);
            assert!(
                !lines.is_empty(),
                "{relative_path}: the assembly holds no body for {}",
                frame.name
            );
            let stack: Vec<&str> = lines
                .iter()
                .copied()
                .filter(|line| ["sp", "chkstk"].iter().any(|word| line.contains(word)))
                .take(24)
                .collect();
            let report = format!(
                "{relative_path} {} {} {}\n{}",
                frame.name,
                frame.bytes,
                frame.kind,
                stack.join("\n")
            );
            println!("large frame, probed={}: {report}", probes(&lines));
            large.push(frame.name.clone());
            if !probes(&lines) {
                unprobed.push(format!("{report}\nprobe arguments: {probe:?}"));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&directory);
    assert!(
        unprobed.is_empty(),
        "runtime frames over a page without a stack probe: {unprobed:#?}\nlarge frames: {large:?}"
    );
}

#[test]
fn a_stack_usage_row_is_read_by_name_size_and_kind() {
    // The third row is an IR unit's, which has no source line: clang names
    // the module by its input path, then the function.
    let report = "file_posix.c:748:5:wf_file_wait_readiness\t32808\tstatic\n\
                  ordinary_values.c:10:1:fill\t64\tdynamic,bounded\n\
                  /scratch/ordinary_values.ll:wf_std.text.host_copy_bytes\t65632\tstatic\n\
                  a line that is not a row\n";
    assert_eq!(
        frames(report),
        [
            Frame {
                name: "wf_file_wait_readiness".to_owned(),
                bytes: 32808,
                kind: "static".to_owned()
            },
            Frame {
                name: "fill".to_owned(),
                bytes: 64,
                kind: "dynamic,bounded".to_owned()
            },
            Frame {
                name: "wf_std.text.host_copy_bytes".to_owned(),
                bytes: 65632,
                kind: "static".to_owned()
            },
        ]
    );
    let needs: Vec<bool> = frames(report).iter().map(Frame::needs_probe).collect();
    assert_eq!(needs, [true, true, true], "a large frame, a dynamic one and an IR unit's");
    let small = frames("a.c:1:1:small\t4096\tstatic\n");
    assert!(!small[0].needs_probe(), "a page exactly stays inside the guard");
}

#[test]
fn a_page_step_with_its_touch_is_a_probe_and_a_lone_store_or_big_step_is_not() {
    let x86_loop = [
        "\tsubq\t$4096, %rsp",
        "\tmovq\t$0, (%rsp)",
        "\tcmpq\t%r11, %rsp",
    ];
    let aarch64_loop = [
        "\tsub\tsp, sp, #1, lsl #12",
        "\tstr\txzr, [sp]",
        "\tcmp\tsp, x9",
    ];
    assert!(probes(&x86_loop));
    assert!(probes(&aarch64_loop));
    assert!(probes(&["\tbl\t___chkstk_darwin"]));
    // A local zeroed at the stack pointer, with the frame taken in one step.
    assert!(!probes(&["\tsubq\t$32808, %rsp", "\tmovq\t$0, (%rsp)"]));
    assert!(!probes(&["\tsub\tsp, sp, #8, lsl #12", "\tstr\txzr, [sp]"]));
    // The step alone, with no touch of what it moved over.
    assert!(!probes(&["\tsubq\t$4096, %rsp"]));
    assert!(!probes(&[]));
}

#[test]
fn a_definition_label_is_found_with_its_comment_and_darwin_prefix() {
    let assembly = "\t.globl f\nf:  # @f\n\tsubq $8, %rsp\n\t.cfi_endproc\ng:\n\tret\n";
    assert_eq!(body(assembly, "f"), ["f:  # @f", "\tsubq $8, %rsp"]);
    let darwin = "_f:  ## @f\n\tsub sp, sp, #16\n\t.cfi_endproc\n";
    assert_eq!(body(darwin, "f"), ["_f:  ## @f", "\tsub sp, sp, #16"]);
    assert!(body(assembly, "missing").is_empty());
}

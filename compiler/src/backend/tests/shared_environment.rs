//! The shared environment of a recursive group
//! (compiler/recursive-environment): the read-only borrows every call
//! inside a group of mutually recursive functions passes on unchanged travel
//! in one record that a call from outside builds, while every symbol another
//! definition can name keeps its ordinary signature.

use super::{compile, compile_and_run, compile_sources, emitted_function};

const PROGRAM: &str = include_str!("../../../../tests/programs/shared_environment.wf");

/// The program with a non-recursive `flat` beside the group, which has the
/// group members' exact signature and is what `main` calls, so `flat`'s
/// ordinary definition is the oracle for what the members' public symbols
/// must look like.
fn with_flat_twin() -> String {
    let twin = r#"
fn flat(left: &Words, right: &Words, scale: &[u64], window: &[u64], depth: u64, at: u64, bias: u64, total: u64) -> result: u64 reads(left), reads(right), reads(scale), reads(window) {
  doc "The members' signature outside their group: one call into it.";
  let through = weigh(left: left, right: right, scale: scale, window: window, depth: depth, at: at, bias: bias, total: total);
  return through;
}
"#;
    let source = PROGRAM.replacen("  let walked = weigh(", "  let walked = flat(", 1);
    assert_ne!(source, PROGRAM, "main must call the group through flat");
    format!("{source}{twin}")
}

/// The first line of one emitted definition.
fn header(function: &str) -> &str {
    function.lines().next().expect("a definition has a header")
}

/// The parameters one definition's header declares.
fn parameters<'header>(header: &'header str, symbol: &str) -> Vec<&'header str> {
    let start = header
        .find(&format!("@{symbol}("))
        .map(|at| at + symbol.len() + 2)
        .unwrap_or_else(|| panic!("{symbol} is not declared by {header}"));
    let end = header
        .rfind(") #")
        .expect("parameters close before attributes");
    header[start..end].split(", ").collect()
}

/// The actual LLVM blocks, including blocks introduced inside an IR block.
fn blocks(function: &str) -> Vec<(&str, String)> {
    let mut blocks: Vec<(&str, String)> = Vec::new();
    for line in function.lines().skip(1) {
        if let Some(label) = line.strip_suffix(':') {
            blocks.push((label, String::new()));
        } else if let Some((_, text)) = blocks.last_mut() {
            text.push_str(line);
            text.push('\n');
        }
    }
    blocks
}

/// Each requested pointer has a real consumer after its load in this block.
/// Follow descriptor reconstruction to its consumer: insertvalue alone, a
/// phi transfer and an instruction mentioned in a comment are not consumers.
fn assert_local_record_consumers(block: &str, consumer: impl Fn(&str) -> bool) {
    let lines: Vec<_> = block
        .lines()
        .map(|line| line.split(';').next().unwrap_or_default())
        .collect();
    for (at, line) in lines.iter().enumerate() {
        if !line.contains(" = load ptr, ptr %wf.env.v") {
            continue;
        }
        let loaded = line.split_whitespace().next().expect("loaded pointer");
        let mut reconstructed = std::collections::BTreeSet::from([loaded]);
        assert!(
            lines[at + 1..].iter().any(|line| {
                if line.contains(" = phi ")
                    || !line
                        .split(|character: char| {
                            character.is_ascii_whitespace()
                                || matches!(character, ',' | '[' | ']' | '(' | ')')
                        })
                        .any(|operand| reconstructed.contains(operand))
                {
                    return false;
                }
                if let Some((result, _)) = line.split_once(" = insertvalue ") {
                    reconstructed.insert(result.trim());
                    false
                } else {
                    consumer(line)
                }
            }),
            "{loaded} must have an actual consumer after its load in {block}"
        );
    }
}

#[test]
fn reconstructed_record_pointer_needs_a_local_consumer() {
    let reconstruction = "  %pointer = load ptr, ptr %wf.env.v0.0.bb1, align 8\n\
                          %partial = insertvalue { ptr, i64 } poison, ptr %pointer, 0\n\
                          %range = insertvalue { ptr, i64 } %partial, i64 4, 1\n";
    for tail in [
        "  br label %next\n",
        "  ; call void @observe({ ptr, i64 } %range)\n  br label %next\n",
        "  %forwarded = phi { ptr, i64 } [ %range, %previous ]\n  br label %next\n",
    ] {
        let block = format!("{reconstruction}{tail}");
        assert!(
            std::panic::catch_unwind(|| assert_local_record_consumers(&block, |_| true)).is_err(),
            "reconstruction without a local consumer must fail: {block}"
        );
    }
    for use_line in [
        "  %count = extractvalue { ptr, i64 } %range, 1\n",
        "  call void @observe({ ptr, i64 } %range)\n",
    ] {
        let block = format!("{reconstruction}{use_line}  br label %next\n");
        assert_local_record_consumers(&block, |_| true);
    }
}

#[test]
fn a_recursive_group_passes_its_unchanged_borrows_in_one_record() {
    let llvm = compile(PROGRAM.as_bytes());
    // Each member's body is its internal form: one record pointer in place
    // of left, right and scale (four words), then window's two words and
    // the four scalars.
    for name in ["weigh", "descend"] {
        let internal = emitted_function(&llvm, &format!("_env_{name}"));
        let declared = header(internal);
        assert!(declared.starts_with("define internal i64 "), "{declared}");
        let parameters = parameters(declared, &format!("wf__env_{name}"));
        assert_eq!(parameters.len(), 7, "{declared}");
        assert!(
            parameters[0].starts_with("ptr noalias readonly nonnull ")
                && parameters[0].ends_with(" align 8 dereferenceable(32) %wf.env"),
            "{declared}"
        );
        assert!(!internal.contains(" @wf_weigh(") && !internal.contains(" @wf_descend("));
    }
    // Calls inside the group pass the caller's own record and name the
    // internal form: one from weigh, and one from each branch of descend.
    let weigh = emitted_function(&llvm, "_env_weigh");
    assert_eq!(
        weigh
            .matches("call i64 @wf__env_descend(ptr %wf.env, ")
            .count(),
        1,
        "{weigh}"
    );
    let descend = emitted_function(&llvm, "_env_descend");
    assert_eq!(
        descend
            .matches("call i64 @wf__env_weigh(ptr %wf.env, ")
            .count(),
        2,
        "{descend}"
    );
    // Each physical block loads a slot at most once. Whole-function counts
    // encoded the former entry-load decision: separate branches can now
    // load the same immutable slot independently.
    for (_, block) in blocks(weigh) {
        let mut fields = std::collections::BTreeSet::new();
        for line in block.lines().filter(|line| {
            line.contains("getelementptr inbounds") && line.contains("ptr %wf.env,")
        }) {
            let field = line.rsplit(", i32 ").next().expect("a record field");
            assert!(fields.insert(field), "slot loaded twice in {block}");
        }
    }
    assert!(weigh.contains("load ptr, ptr %wf.env.v"), "{weigh}");
    assert!(weigh.contains("load i64, ptr %wf.env.v"), "{weigh}");
    // The public symbol is the entry from outside: it keeps external
    // linkage, builds the record of the three unchanged borrows in its own
    // frame (window, which descend narrows on one edge, stays out) and calls
    // the internal form.
    for name in ["weigh", "descend"] {
        let public = emitted_function(&llvm, name);
        assert!(header(public).starts_with("define i64 "), "{public}");
        assert!(
            public.contains("%wf.env = alloca { ptr, ptr, ptr, i64 }, align 8"),
            "{public}"
        );
        assert_eq!(public.matches("store ").count(), 4, "{public}");
        assert!(
            public.contains(&format!("call i64 @wf__env_{name}(ptr %wf.env, ")),
            "{public}"
        );
    }
    let main = emitted_function(&llvm, "main");
    assert!(main.contains("@wf_weigh("), "{main}");
    assert!(!main.contains("@wf__env_"), "{main}");
}

#[test]
fn a_record_slot_is_loaded_only_in_the_branch_that_uses_it() {
    let llvm = compile(PROGRAM.as_bytes());
    let weigh = emitted_function(&llvm, "_env_weigh");
    let blocks = blocks(weigh);
    let (_, entry) = blocks.first().expect("weigh has an entry block");
    // depth == 0 returns total without reading any recorded borrow. The
    // old emitter loaded every slot at entry, so this assertion fails before
    // block-local definitions even though the nonzero branch needs left.
    assert!(!entry.contains("ptr %wf.env"), "{entry}");
    let left_load = blocks.iter().skip(1).find(|(_, block)| {
        block.lines().any(|line| {
            line.contains("getelementptr inbounds")
                && line.contains("ptr %wf.env,")
                && line.ends_with(", i32 0, i32 0")
        })
    });
    let (_, using_branch) = left_load.expect("the nonzero branch reads left's record slot");
    let lines: Vec<_> = using_branch.lines().collect();
    let address = lines
        .iter()
        .find(|line| {
            line.contains("getelementptr inbounds")
                && line.contains("ptr %wf.env,")
                && line.ends_with(", i32 0, i32 0")
        })
        .expect("left's field address")
        .split_whitespace()
        .next()
        .expect("field address result");
    let (load_at, loaded) = lines
        .iter()
        .enumerate()
        .find_map(|(at, line)| {
            line.contains(&format!(" = load ptr, ptr {address},"))
                .then(|| (at, line.split_whitespace().next().expect("loaded pointer")))
        })
        .expect("left's pointer is loaded in its using branch");
    assert!(
        lines[load_at + 1..].iter().any(|line| {
            line.split(|character: char| {
                character.is_ascii_whitespace()
                    || matches!(character, ',' | '[' | ']' | '(' | ')')
            })
            .any(|operand| operand == loaded)
        }),
        "left's loaded pointer must be consumed later in the same block: {using_branch}"
    );
    let early_return = blocks.iter().find(|(_, block)| {
        block.lines().any(|line| {
            line.trim_start()
                .strip_prefix("ret i64 ")
                .is_some_and(|operand| traced(weigh, operand) == "%v7")
        })
    });
    let (_, unused_branch) = early_return.expect("depth == 0 returns total");
    assert!(!unused_branch.contains("ptr %wf.env"), "{unused_branch}");
    for (_, block) in &blocks {
        assert_local_record_consumers(block, |line| !line.contains(" = phi "));
    }
}

#[test]
fn recorded_array_borrows_load_only_where_equality_snapshots_use_them() {
    let source = r#"fn compare_first(left: &Array<u64, SIZE>, right: &Array<u64, SIZE>, spare: &u64, depth: u64, x: u64, y: u64, z: u64, p: u64, q: u64) -> result: Bool reads(left), reads(right), reads(spare) {
  doc "Compare borrowed arrays after the recursive chain ends.";
  if depth == 0_u64 {
    return left^ == right^;
  }
  let below = depth -wrap 1_u64;
  let same = compare_second(left: left, right: right, spare: spare, depth: below, x: x, y: y, z: z, p: p, q: q);
  return same;
}

fn compare_second(left: &Array<u64, SIZE>, right: &Array<u64, SIZE>, spare: &u64, depth: u64, x: u64, y: u64, z: u64, p: u64, q: u64) -> result: Bool reads(left), reads(right), reads(spare) {
  doc "Forward the same borrowed arrays back to the first member.";
  let same = compare_first(left: left, right: right, spare: spare, depth: depth, x: x, y: y, z: z, p: p, q: q);
  return same;
}

fn main() -> status: std::process::ExitStatus pure {
  let left = array_filled::<u64, SIZE>(value: 11_u64);
  let right = array_filled::<u64, SIZE>(value: 11_u64);
  let spare = 0_u64;
  let same = compare_first(left: &left, right: &right, spare: &spare, depth: 2_u64, x: 1_u64, y: 2_u64, z: 3_u64, p: 4_u64, q: 5_u64);
  if same {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#;
    for size in [0, 2] {
        let source = source.replace("SIZE", &size.to_string());
        let llvm = compile(source.as_bytes());
        let first = emitted_function(&llvm, "_env_compare_first");
        let loads: Vec<_> = blocks(first)
            .into_iter()
            .filter(|(_, block)| block.contains("load ptr, ptr %wf.env.v"))
            .collect();
        // Equality consumes by-value snapshots. The storage planner copies
        // them before comparison, including zero-byte copies for empty
        // arrays; those memmoves really use the two borrowed pointers.
        // Forwarding-only blocks and the unused spare must load nothing.
        assert_eq!(loads.len(), 1, "{first}");
        let (_, body) = &loads[0];
        assert_eq!(body.matches("load ptr, ptr %wf.env.v").count(), 2, "{body}");
        assert_eq!(body.matches("call void @llvm.memmove.").count(), 2, "{body}");
        assert_local_record_consumers(body, |line| line.contains("call void @llvm.memmove."));
        assert!(!first.contains("%wf.env.v2."), "{first}");
        if size == 0 {
            assert!(!first.contains(".body:"), "{first}");
        } else {
            assert!(first.contains("getelementptr inbounds [2 x i64]"), "{first}");
        }
        let second = emitted_function(&llvm, "_env_compare_second");
        assert!(!second.contains("load ptr, ptr %wf.env.v"), "{second}");
    }
}

#[test]
fn recorded_loop_capture_loads_in_the_indexed_driver_call_block() {
    let source = br#"fn count_first(left: &u64, right: &u64, spare: &u64, depth: u64, x: u64, y: u64, z: u64, p: u64, q: u64) -> result: u64 reads(left), reads(right), reads(spare) {
  doc "The recursive edge forwards borrows; the base case uses them only in an indexed loop.";
  if depth != 0_u64 {
    let below = depth -wrap 1_u64;
    let counted = count_second(left: left, right: right, spare: spare, depth: below, x: x, y: y, z: z, p: p, q: q);
    return counted;
  }
  let counts = box_array_filled::<u64>(count: 8_u64, value: 0_u64);
  for (i in 0_u64..64_u64) {
    let bucket = i % 8_u64;
    let added = left^ +wrap right^;
    set counts.inner[bucket] = counts.inner[bucket] +wrap added;
  }
  return counts.inner[0_u64];
}

fn count_second(left: &u64, right: &u64, spare: &u64, depth: u64, x: u64, y: u64, z: u64, p: u64, q: u64) -> result: u64 reads(left), reads(right), reads(spare) {
  doc "Close the cycle without otherwise consuming the borrowed words.";
  let counted = count_first(left: left, right: right, spare: spare, depth: depth, x: x, y: y, z: z, p: p, q: q);
  return counted;
}

fn main() -> status: std::process::ExitStatus pure {
  let left = 3_u64;
  let right = 2_u64;
  let spare = 0_u64;
  let counted = count_first(left: &left, right: &right, spare: &spare, depth: 2_u64, x: 1_u64, y: 2_u64, z: 3_u64, p: 4_u64, q: 5_u64);
  if counted != 40_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = super::emit_lowered(
        source,
        crate::OverlapLowering::OnWithRecursionBudget {
            budget: crate::RecursionBudget::Off,
            call_grain: crate::CallGrain::WorkUnit,
            sequential_refusal: false,
        },
    );
    let first = emitted_function(&llvm, "_env_count_first");
    // The indexed loop keeps its range driver; the site itself allocates
    // nothing and asks the runtime for no split budget.
    assert!(first.contains("@wf__par_split_count_first."), "{first}");
    assert!(!first.contains("@wf__par_split_budget("), "{first}");
    assert!(!first.contains("@wf__heap_take("), "{first}");
    assert!(
        llvm.contains(".allocation = call ptr @wf__heap_take("),
        "the driver's granted far half owns the private contribution"
    );
    let loads: Vec<_> = blocks(first)
        .into_iter()
        .filter(|(_, block)| block.contains("load ptr, ptr %wf.env.v"))
        .collect();
    // The driver takes every record slot as a capture (left, right and
    // spare), so each load's use is its operand of the driver call; each slot
    // is loaded exactly once, in the call's block, and nowhere else.
    assert_eq!(loads.len(), 1, "{first}");
    let (_, body) = &loads[0];
    assert_eq!(body.matches("load ptr, ptr %wf.env.v").count(), 3, "{body}");
    assert!(body.contains("@wf__par_split_count_first."), "{body}");
    assert_local_record_consumers(body, |line| line.contains("@wf__par_split_count_first."));
}

#[test]
fn a_group_member_keeps_the_public_signature_of_an_ordinary_definition() {
    let llvm = compile_sources(&[("test.wf", with_flat_twin().as_bytes())]);
    let flat = header(emitted_function(&llvm, "flat")).to_owned();
    assert!(flat.starts_with("define i64 @wf_flat("), "{flat}");
    for name in ["weigh", "descend"] {
        let public =
            header(emitted_function(&llvm, name)).replace(&format!("@wf_{name}("), "@wf_flat(");
        assert_eq!(public, flat);
    }
    // A call from outside the group names the public symbol.
    let twin = emitted_function(&llvm, "flat");
    assert!(twin.contains("call i64 @wf_weigh("), "{twin}");
    assert!(!twin.contains("@wf__env_"), "{twin}");

    // The program's own result check, now entered through flat.
    let output = compile_and_run(&llvm);
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn a_borrow_passed_on_unchanged_on_every_edge_joins_the_record() {
    // The same group with descend passing window on unchanged in both
    // branches: window now joins the record, so it was the narrowed edge
    // alone that kept it out above.
    let source = PROGRAM
        .replacen("      let rest = &window^[start..count];\n", "", 1)
        .replacen("window: rest,", "window: window,", 1);
    assert!(
        !source.contains("window^[start..count]") && !source.contains("window: rest"),
        "the variant must drop the narrowed borrow"
    );
    let llvm = compile_sources(&[("test.wf", source.as_bytes())]);
    let public = emitted_function(&llvm, "weigh");
    assert!(
        public.contains("%wf.env = alloca { ptr, ptr, ptr, i64, ptr, i64 }, align 8"),
        "{public}"
    );
    let internal = emitted_function(&llvm, "_env_weigh");
    assert_eq!(
        parameters(header(internal), "wf__env_weigh").len(),
        5,
        "{internal}"
    );
}

#[test]
fn a_group_whose_arguments_fit_the_registers_keeps_the_ordinary_form() {
    // Four argument words: every one already travels in a register on every
    // admitted target, so the record could only add loads.
    let source = br#"struct Words {
  list: Box<Array<u64>>;
}

fn ping(words: &Words, scale: &[u64], depth: u64) -> result: u64 reads(words), reads(scale) {
  doc "One half of a short mutual recursion.";
  if depth == 0_u64 {
    return 0_u64;
  }
  let next = depth -wrap 1_u64;
  let below = pong(words: words, scale: scale, depth: next);
  let more = below +wrap 1_u64;
  return more;
}

fn pong(words: &Words, scale: &[u64], depth: u64) -> result: u64 reads(words), reads(scale) {
  doc "The other half.";
  let below = ping(words: words, scale: scale, depth: depth);
  return below;
}

fn main() -> status: std::process::ExitStatus pure {
  let list = box_array_filled::<u64>(count: 4_u64, value: 0_u64);
  let words = Words(list: move list);
  let values = array_filled::<u64, 2>(value: 0_u64);
  let walked = ping(words: &words, scale: &values[0_u64..2_u64], depth: 9_u64);
  if walked != 9_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    assert!(!llvm.contains("@wf__env_"), "{llvm}");
    // Outside the environment form no call of the group is kept out of
    // line. (The host entry's own call of the program body is `noinline`
    // in every module and is no call of the group.)
    assert!(
        !llvm.lines().any(|line| {
            line.contains(" call ")
                && (line.contains(" @wf_ping(") || line.contains(" @wf_pong("))
                && line.ends_with(" noinline")
        }),
        "{llvm}"
    );
    assert!(emitted_function(&llvm, "ping").contains("call i64 @wf_pong("));
}

#[test]
fn a_fragment_build_defines_each_internal_form_once() {
    let module = crate::compile(
        &[crate::SourceInput::new("test.wf", PROGRAM.as_bytes())],
        crate::CompilerLimits::default(),
    )
    .expect("the program compiles");
    let fragments = crate::split_module(&module, crate::FragmentGranularity::Function)
        .expect("the module splits");
    for symbol in [
        "@wf__env_weigh(",
        "@wf__env_descend(",
        "@wf_weigh(",
        "@wf_descend(",
    ] {
        let definitions = fragments
            .iter()
            .filter(|fragment| {
                fragment
                    .lines()
                    .any(|line| line.starts_with("define ") && line.contains(symbol))
            })
            .count();
        assert_eq!(definitions, 1, "{symbol}");
    }
}

/// The selector chain's shape: step calls compound, compound calls probe,
/// probe calls step again. probe is defined first, as in Snowghost's witness,
/// and main enters the chain at step. As there, each member has parameters
/// of its own (probe's simple, compound's span, step's first and last)
/// ahead of id and depth, which every call inside the group forwards.
const CHAIN: &str = r#"struct Words {
  list: Box<Array<u64>>;
}

fn probe(rules: &Words, classes: &Words, tags: &Words, table: &[u64], simple: u64, id: u64, depth: u64) -> result: Bool reads(rules), reads(classes), reads(tags), reads(table) {
  doc "A leaf probe of one simple selector, except that it negates a nested walk while depth lasts.";
  if depth == 0_u64 {
    if id < tags^.list.inner.len {
      let tag = tags^.list.inner[id];
      let same = tag == simple;
      return same;
    }
    return False();
  }
  let below = depth -wrap 1_u64;
  let from = simple +wrap 1_u64;
  let to = from +wrap 1_u64;
  let inner = step(rules: rules, classes: classes, tags: tags, table: table, first: from, last: to, id: id, depth: below);
  let negated = bnot(inner);
  return negated;
}

fn compound(rules: &Words, classes: &Words, tags: &Words, table: &[u64], span: u64, id: u64, depth: u64) -> result: Bool reads(rules), reads(classes), reads(tags), reads(table) {
  doc "Every probe of a span.";
  for (at in 0_u64..span) {
    let ok = probe(rules: rules, classes: classes, tags: tags, table: table, simple: at, id: id, depth: depth);
    if ok {
    } else {
      return False();
    }
  }
  return True();
}

fn step(rules: &Words, classes: &Words, tags: &Words, table: &[u64], first: u64, last: u64, id: u64, depth: u64) -> result: Bool reads(rules), reads(classes), reads(tags), reads(table) {
  doc "The walk's entry.";
  let has_any = first < last;
  if has_any {
  } else {
    return False();
  }
  let span = last -wrap first;
  let subject = compound(rules: rules, classes: classes, tags: tags, table: table, span: span, id: id, depth: depth);
  return subject;
}

fn main() -> status: std::process::ExitStatus pure {
  let rule_list = box_array_filled::<u64>(count: 4_u64, value: 1_u64);
  let rules = Words(list: move rule_list);
  let class_list = box_array_filled::<u64>(count: 4_u64, value: 1_u64);
  let classes = Words(list: move class_list);
  let tag_list = box_array_filled::<u64>(count: 4_u64, value: 1_u64);
  let tags = Words(list: move tag_list);
  let none = array_filled::<u64, 1>(value: 0_u64);
  let matched = step(rules: &rules, classes: &classes, tags: &tags, table: &none[0_u64..0_u64], first: 1_u64, last: 2_u64, id: 2_u64, depth: 2_u64);
  if matched {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#;

/// The call lines of one emitted definition that name `callee`.
fn calls_to<'function>(function: &'function str, callee: &str) -> Vec<&'function str> {
    let named = format!(" @{callee}(");
    function
        .lines()
        .filter(|line| line.contains(" call ") && line.contains(&named))
        .collect()
}

#[test]
fn only_the_call_that_closes_the_cycle_is_kept_out_of_line() {
    let llvm = compile(CHAIN.as_bytes());
    // The walk from step, where main enters, runs forward through compound
    // into probe; probe's call back into step closes the cycle.
    let probe = emitted_function(&llvm, "_env_probe");
    let back = calls_to(probe, "wf__env_step");
    assert_eq!(back.len(), 1, "{probe}");
    assert!(back[0].ends_with(") noinline"), "{probe}");
    for (caller, callee) in [("step", "compound"), ("compound", "probe")] {
        let function = emitted_function(&llvm, &format!("_env_{caller}"));
        let forward = calls_to(function, &format!("wf__env_{callee}"));
        assert!(!forward.is_empty(), "{function}");
        assert!(
            forward.iter().all(|line| !line.contains("noinline")),
            "{function}"
        );
    }
    // Calls from outside the group, and the entries' calls of their
    // internal forms, stay inlinable: of every call naming a member, public
    // or internal, only the back edge is marked. (The host entry's own call
    // of the program body is `noinline` in every module and names no member.)
    let marked = llvm
        .lines()
        .filter(|line| {
            line.contains(" call ")
                && ["probe", "compound", "step"].iter().any(|name| {
                    line.contains(&format!(" @wf_{name}("))
                        || line.contains(&format!(" @wf__env_{name}("))
                })
                && line.ends_with(" noinline")
        })
        .count();
    assert_eq!(marked, 1, "{llvm}");
}

#[test]
fn forwarded_parameters_take_the_same_position_in_every_internal_form() {
    let llvm = compile(CHAIN.as_bytes());
    // After the record pointer, id then depth, whatever each member declares
    // before them in source: probe's are its parameters 5 and 6, compound's
    // 5 and 6, step's 6 and 7.
    for (name, id, depth, rest) in [
        ("probe", 5, 6, &["i64 %v4"][..]),
        ("compound", 5, 6, &["i64 %v4"][..]),
        ("step", 6, 7, &["i64 %v4", "i64 %v5"][..]),
    ] {
        let internal = header(emitted_function(&llvm, &format!("_env_{name}")));
        let declared = parameters(internal, &format!("wf__env_{name}"));
        assert!(declared[0].ends_with(" %wf.env"), "{internal}");
        assert_eq!(declared[1], format!("i64 %v{id}"), "{internal}");
        assert_eq!(declared[2], format!("i64 %v{depth}"), "{internal}");
        assert_eq!(&declared[3..], rest, "{internal}");
    }
    // The public symbol keeps the source order of the ordinary ABI.
    let public = header(emitted_function(&llvm, "step"));
    let declared = parameters(public, "wf_step");
    assert_eq!(
        &declared[declared.len() - 4..],
        &["i64 %v4", "i64 %v5", "i64 %v6", "i64 %v7"],
        "{public}"
    );
    // A call inside the group passes its arguments in the callee's order:
    // step hands compound its own id and depth right after the record, and
    // its span after them. The body may carry a formal through block
    // parameters, so each operand is traced back through phis whose every
    // incoming value is one operand.
    let step = emitted_function(&llvm, "_env_step");
    let calls = calls_to(step, "wf__env_compound");
    assert_eq!(calls.len(), 1, "{step}");
    let arguments = call_operands(calls[0]);
    assert_eq!(arguments.len(), 4, "{step}");
    assert_eq!(arguments[0], "%wf.env", "{step}");
    assert_eq!(traced(step, arguments[1]), "%v6", "{step}");
    assert_eq!(traced(step, arguments[2]), "%v7", "{step}");
    assert!(
        !["%v4", "%v5", "%v6", "%v7"].contains(&traced(step, arguments[3])),
        "{step}"
    );
}

#[test]
fn full_forwarded_classes_precede_classes_missing_from_a_member() {
    let source = br#"struct Words {
  list: Box<Array<u64>>;
}

fn first(left: &Words, right: &Words, scale: &[u64], spare: &[u64], x: u64, id: u64, depth: u64) -> result: u64 reads(left), reads(right), reads(scale), reads(spare) {
  doc "Forward x and id to the second member.";
  if depth == 0_u64 {
    let total = x +wrap id;
    return total;
  }
  let below = depth -wrap 1_u64;
  let result = second(left: left, right: right, scale: scale, spare: spare, x: x, id: id, depth: below);
  return result;
}

fn second(left: &Words, right: &Words, scale: &[u64], spare: &[u64], x: u64, id: u64, depth: u64) -> result: u64 reads(left), reads(right), reads(scale), reads(spare) {
  doc "Only id reaches the third member.";
  if depth == 0_u64 {
    let total = x +wrap id;
    return total;
  }
  let below = depth -wrap 1_u64;
  let result = third(left: left, right: right, scale: scale, spare: spare, id: id, depth: below);
  return result;
}

fn third(left: &Words, right: &Words, scale: &[u64], spare: &[u64], id: u64, depth: u64) -> result: u64 reads(left), reads(right), reads(scale), reads(spare) {
  doc "Close the cycle with a fresh x and the same id.";
  if depth == 0_u64 {
    return id;
  }
  let below = depth -wrap 1_u64;
  let result = first(left: left, right: right, scale: scale, spare: spare, x: 0_u64, id: id, depth: below);
  return result;
}

fn main() -> status: std::process::ExitStatus pure {
  let list = box_array_filled::<u64>(count: 1_u64, value: 0_u64);
  let words = Words(list: move list);
  let values = array_filled::<u64, 1>(value: 0_u64);
  let walked = first(left: &words, right: &words, scale: &values[0_u64..1_u64], spare: &values[0_u64..1_u64], x: 7_u64, id: 9_u64, depth: 2_u64);
  if walked != 9_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    // x's partial class starts at first's parameter 4, before id's full
    // class at parameter 5. Ordering solely by that index puts id second
    // after the record in first and second, but first in third. Full
    // classes must come first so id always immediately follows the record.
    for (name, id, rest) in [
        ("first", 5, &["i64 %v4", "i64 %v6"][..]),
        ("second", 5, &["i64 %v4", "i64 %v6"][..]),
        ("third", 4, &["i64 %v5"][..]),
    ] {
        let internal = header(emitted_function(&llvm, &format!("_env_{name}")));
        let declared = parameters(internal, &format!("wf__env_{name}"));
        assert!(declared[0].ends_with(" %wf.env"), "{internal}");
        assert_eq!(declared[1], format!("i64 %v{id}"), "{internal}");
        assert_eq!(&declared[2..], rest, "{internal}");
    }
    // Both a call with x and a call without it keep id in that first
    // position, while first passes x in second's partial-class position.
    for (caller, callee, count) in [("first", "second", 4), ("second", "third", 3)] {
        let function = emitted_function(&llvm, &format!("_env_{caller}"));
        let calls = calls_to(function, &format!("wf__env_{callee}"));
        assert_eq!(calls.len(), 1, "{function}");
        let arguments = call_operands(calls[0]);
        assert_eq!(arguments.len(), count, "{function}");
        assert_eq!(arguments[0], "%wf.env", "{function}");
        assert_eq!(traced(function, arguments[1]), "%v5", "{function}");
        if caller == "first" {
            assert_eq!(traced(function, arguments[2]), "%v4", "{function}");
        }
    }
}

/// The operand names one call line passes, without their types.
fn call_operands(line: &str) -> Vec<&str> {
    let start = line.find('(').expect("a call has arguments") + 1;
    let end = line.rfind(')').expect("a call's arguments close");
    line[start..end]
        .split(", ")
        .map(|argument| argument.rsplit(' ').next().expect("an operand"))
        .collect()
}

/// The value `name` stands for in `function`, followed back through phis
/// whose every incoming value is the same operand.
fn traced<'function>(function: &'function str, name: &'function str) -> &'function str {
    let mut name = name;
    loop {
        let definition = format!("  {name} = phi ");
        let Some(line) = function.lines().find(|line| line.starts_with(&definition)) else {
            return name;
        };
        let incoming: Vec<&str> = line
            .split('[')
            .skip(1)
            .filter_map(|edge| edge.trim().split(',').next())
            .map(str::trim)
            .collect();
        match incoming.as_slice() {
            [first, rest @ ..] if *first != name && rest.iter().all(|other| other == first) => {
                name = *first;
            }
            _ => return name,
        }
    }
}

#[test]
fn functions_without_a_record_resolve_their_operands_before_their_blocks() {
    // No recursive group: nothing here may take the record's deferred path,
    // so every operand's place is resolved where the operation starts, before
    // the blocks the comparison, the probe and the splitter open.
    let source = br#"fn same(left: &Array<u64, 2>, right: &Array<u64, 2>) -> result: Bool reads(left), reads(right) {
  doc "Array equality over borrowed arrays.";
  return left^ == right^;
}

fn count_words(left: &u64, right: &u64) -> result: u64 reads(left), reads(right) {
  doc "An indexed loop the range driver takes, capturing two borrows.";
  let counts = box_array_filled::<u64>(count: 8_u64, value: 0_u64);
  for (i in 0_u64..64_u64) {
    let bucket = i % 8_u64;
    let added = left^ +wrap right^;
    set counts.inner[bucket] = counts.inner[bucket] +wrap added;
  }
  return counts.inner[0_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u64, 2>(value: 7_u64);
  let b = array_filled::<u64, 2>(value: 7_u64);
  let data = box_array_filled::<u8>(count: 64_u64, value: 97_u8);
  let mark = 88_u8;
  let seen = 0_u64;
  let stop = data.inner.len;
  let cursor = 0_u64;
  loop @walk {
    let done = cursor >= stop;
    if done {
      break @walk;
    }
    let byte = data.inner[cursor];
    let newline = byte == 10_u8;
    if newline {
      set seen = seen +wrap 1_u64;
    }
    let lead = byte == mark;
    if lead {
      set seen = seen +wrap 2_u64;
    }
    set cursor = cursor +wrap 1_u64;
  }
  let one = 3_u64;
  let two = 2_u64;
  let counted = count_words(left: &one, right: &two);
  if same(left: &a, right: &b) {
    if counted == 40_u64 {
      return std::process::exit_status(code: 0_u8);
    }
  }
  return std::process::exit_status(code: 1_u8);
}
"#;
    let llvm = super::emit_lowered(
        source,
        crate::OverlapLowering::OnWithRecursionBudget {
            budget: crate::RecursionBudget::Off,
            call_grain: crate::CallGrain::WorkUnit,
            sequential_refusal: false,
        },
    );
    assert!(!llvm.contains("wf.env"), "{llvm}");

    // Every pointer a block's address computations start from is defined
    // outside that block (a parameter, an entry allocation or an earlier
    // block), never by a place computed inside the comparison loop's body.
    let same = emitted_function(&llvm, "same");
    let body = blocks(same)
        .into_iter()
        .find(|(label, _)| label.ends_with(".body"))
        .expect("array equality loops over its elements");
    for line in body.1.lines().filter(|line| line.contains("getelementptr")) {
        let base = line
            .split("ptr ")
            .nth(1)
            .and_then(|rest| rest.split(',').next())
            .expect("an element address names its base");
        assert!(
            !body.1.contains(&format!("  {base} = ")),
            "{base} in {same}"
        );
    }

    // The driver's operands taken from the function body exist before its
    // call. The site prepares no private storage and asks for no budget.
    let count = emitted_function(&llvm, "count_words");
    assert!(!count.contains("@wf__par_split_budget("), "{count}");
    assert!(!count.contains("@wf__heap_take("), "{count}");
    let call = count
        .lines()
        .find(|line| line.contains("call ") && line.contains("@wf__par_split_count_words."))
        .expect("the loop is split");
    let call_at = count.find(call).expect("the call line is in the body");
    let body_operands: Vec<_> = call_operands(call)
        .into_iter()
        .filter(|op| op.starts_with("%"))
        .collect();
    assert!(
        body_operands.len() >= 3,
        "the captures and the counts array: {call}"
    );
    for operand in body_operands {
        let defined = count
            .find(&format!("  {operand} = "))
            .or_else(|| header(count).contains(&format!(" {operand}")).then_some(0));
        assert!(
            defined.is_some_and(|at| at < call_at),
            "{operand} is defined before the driver call in {count}"
        );
    }

    // The probe was recognized: its wide load block exists.
    assert!(llvm.contains("buffer.probe.load."), "{llvm}");
}

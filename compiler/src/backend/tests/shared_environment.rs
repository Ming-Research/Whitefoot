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
    // The body loads each slot once from the record, under the names an
    // ordinary body receives it by.
    assert_eq!(
        weigh.matches("load ptr, ptr %wf.env.v").count(),
        3,
        "{weigh}"
    );
    assert_eq!(
        weigh.matches("load i64, ptr %wf.env.v").count(),
        1,
        "{weigh}"
    );
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
    // Outside the environment form no call is kept out of line.
    assert!(
        !llvm
            .lines()
            .any(|line| line.contains(" call ") && line.ends_with(" noinline")),
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
/// and main enters the chain at step.
const CHAIN: &str = r#"struct Words {
  list: Box<Array<u64>>;
}

fn probe(rules: &Words, classes: &Words, tags: &Words, table: &[u64], first: u64, last: u64, id: u64, depth: u64) -> result: Bool reads(rules), reads(classes), reads(tags), reads(table) {
  doc "A leaf probe, except that it negates a nested walk while depth lasts.";
  if depth == 0_u64 {
    if id < tags^.list.inner.len {
      let tag = tags^.list.inner[id];
      let same = tag == first;
      return same;
    }
    return False();
  }
  let below = depth -wrap 1_u64;
  let inner = step(rules: rules, classes: classes, tags: tags, table: table, first: first, last: last, id: id, depth: below);
  let negated = bnot(inner);
  return negated;
}

fn compound(rules: &Words, classes: &Words, tags: &Words, table: &[u64], first: u64, last: u64, id: u64, depth: u64) -> result: Bool reads(rules), reads(classes), reads(tags), reads(table) {
  doc "Every probe of an interval.";
  for (at in first..last) {
    let ok = probe(rules: rules, classes: classes, tags: tags, table: table, first: at, last: last, id: id, depth: depth);
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
  let subject = compound(rules: rules, classes: classes, tags: tags, table: table, first: first, last: last, id: id, depth: depth);
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
    // internal forms, stay inlinable.
    let marked = llvm
        .lines()
        .filter(|line| line.contains(" call ") && line.ends_with(" noinline"))
        .count();
    assert_eq!(marked, 1, "{llvm}");
}

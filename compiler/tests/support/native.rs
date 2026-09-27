//! Shared source geometries and forwarding observers for native compiler
//! checks. These do not alter production scheduling or source acceptance.

pub(crate) fn spine_source(depth: u64) -> Vec<u8> {
    format!(
        r#"fn leafval(v: f64) -> result: f64 pure {{
  return fmul.strict(v, 0.5_f64);
}}

fn spine(depth: u64, v: f64) -> result: f64 pure {{
  let done = depth == 0_u64;
  if done {{
    return v;
  }}
  let below = depth -wrap 1_u64;
  let a = spine(depth: below, v: v);
  let b = leafval(v: v);
  return fadd.strict(a, b);
}}

fn main() -> status: std::process::ExitStatus pure {{
  let total = spine(depth: {depth}_u64, v: 1.0009765625_f64);
  let bits = reinterpret::<f64, u64>(total);
  let low = iand(bits, 1_u64);
  match cvt.checked::<u64, u8>(low) {{
    Ok(value: byte) => {{
      return std::process::exit_status(code: byte);
    }}
    Err(error: wide) => {{
      return std::process::exit_status(code: 9_u8);
    }}
  }}
}}
"#
    )
    .into_bytes()
}

pub(crate) fn wide_frame_source(depth: u64, slots: u64) -> Vec<u8> {
    format!(
        r#"fn spine(depth: u64, v: u64, i: u8) -> result: u64 pure {{
  let pad = slots_new::<u64, {slots}>();
  for @fill (
    at in 0_u64..{slots}_u64,
    invariant grown: pad.len >= at,
    invariant spare: pad.cap + at >= pad.len + {slots}_u64
  ) {{
    let seed = v +wrap at;
    let square = seed *wrap seed;
    place_back(window: &pad, value: square);
  }}
  let wide = cvt::<u8, u64>(i);
  set pad[wide] = depth;
  let done = depth == 0_u64;
  if done {{
    return pad[wide];
  }}
  let below = depth -wrap 1_u64;
  let a = spine(depth: below, v: v, i: i);
  let after = a % {slots}_u64;
  let b = pad[after];
  return a +wrap b;
}}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {{
  let std::process::Inputs(args: args, cwd: cwd, stdout: unused_stdout, stderr: unused_stderr, handles: factory, stdin: unused_stdin) = move inputs;
  std::fs::close_directory(factory: &factory, directory: move cwd);
  let count = 0_u64;
  set count = std::text::args_count(args: &args);
  match cvt.checked::<u64, u8>(count) {{
    Ok(value: idx) => {{
      let depth = count *wrap {depth}_u64;
      let r = spine(depth: depth, v: 3_u64, i: idx);
      let ok = r > 0_u64;
      if ok {{
        return std::process::exit_status(code: 0_u8);
      }}
      return std::process::exit_status(code: 1_u8);
    }}
    Err(error: e) => {{
      return std::process::exit_status(code: 9_u8);
    }}
  }}
}}
"#
    )
    .into_bytes()
}

pub(crate) fn observe_worker_schedule(module: &str) -> String {
    format!(
        "{}\ndeclare void @wf_test_worker_publish(ptr, ptr)\ndeclare void @wf_test_worker_join(ptr)\n",
        module
            .replace(
                "call void @wf__par_publish(",
                "call void @wf_test_worker_publish("
            )
            .replace(
                "call void @wf__par_join(",
                "call void @wf_test_worker_join("
            )
    )
}

pub(crate) fn observe_layout(llvm: &str) -> (String, String) {
    let mut observed = observe_worker_schedule(llvm);
    for (index, name) in ["layout", "layout_banded"].iter().enumerate() {
        let signature = format!(" @wf_{name}(");
        let definition = observed
            .lines()
            .find(|line| line.starts_with("define ") && line.contains(&signature))
            .expect("layout function definition");
        let start = observed.find(definition).expect("layout body start");
        let end = start + observed[start..].find("\n}").expect("layout body end") + 2;
        let body = observed[start..end].to_owned();
        let entry = body
            .lines()
            .find(|line| line.ends_with(':'))
            .expect("entry block");
        let replacement = body
            .replacen(
                entry,
                &format!("{entry}\n  call void @wf_test_worker_schedule_begin()"),
                1,
            )
            .replace(
                "  ret double ",
                &format!("  call void @wf_test_layout_end(i32 {index})\n  ret double "),
            );
        assert!(replacement.contains(&format!("@wf_test_layout_end(i32 {index})")));
        observed = observed.replacen(&body, &replacement, 1);
    }
    observed.push_str(
        "\ndeclare void @wf_test_worker_schedule_begin()\ndeclare void @wf_test_layout_end(i32)\n",
    );
    const WORKER_SCHEDULE: &str = include_str!("../../src/backend/tests/worker_schedule.c");
    let host = format!(
        "#define WF_TEST_SCHEDULE_MANUAL\n{WORKER_SCHEDULE}\n{}",
        r#"
static unsigned folds;
void wf_test_layout_end(unsigned which) {
    if (which != folds || !atomic_load(&schedule_entered)) {
        fprintf(stderr, "layout fold %u did not enter a real worker\n", which);
        exit(116);
    }
    ++folds;
    wf_test_worker_schedule_end();
}
static void report(void) { if (folds != 2) { fputs("missing layout fold\n", stderr); _Exit(117); } }
__attribute__((constructor)) static void observe(void) { atexit(report); }
"#
    );
    (observed, host)
}

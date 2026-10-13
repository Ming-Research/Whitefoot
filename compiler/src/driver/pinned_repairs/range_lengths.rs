//! Call-formed range length repairs, consumed in their original order by
//! the shared repair harness. Keep these programs while these repairs print.

use super::RepairPair;

pub(super) const RANGE_LENGTHS: &[RepairPair] = &[
    RepairPair {
        name: "call-requirement-over-a-range-formed-at-the-call.wf",
        rejected: br#"fn need(v: &[u64]) -> result: u64 pure contract {
  requires 2_u64 <= v^.len;
} {
  return 0_u64;
}

fn caller(k: u64) -> result: u64 pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  if k <= 4_u64 {
    let r = need(v: &values[0_u64..k]);
    return r;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(k: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `values[0_u64..k].len` is `k` [REF-4], so this call needs `2_u64 <= k`: guard the call with `if 2_u64 <= k` where skipping it is the intended behavior; or bind the range with one preceding `let`, use the binding in the call, and establish the relation over the binding\n",
        ],
        repaired: &[
            br#"fn need(v: &[u64]) -> result: u64 pure contract {
  requires 2_u64 <= v^.len;
} {
  return 0_u64;
}

fn caller(k: u64) -> result: u64 pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  if k <= 4_u64 {
    if 2_u64 <= k {
      let r = need(v: &values[0_u64..k]);
      return r;
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(k: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn need(v: &[u64]) -> result: u64 pure contract {
  requires 2_u64 <= v^.len;
} {
  return 0_u64;
}

fn caller(k: u64) -> result: u64 pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  if k <= 4_u64 {
    let part = &values[0_u64..k];
    if 2_u64 <= part^.len {
      let r = need(v: part);
      return r;
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(k: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "call-requirement-over-a-range-length-that-is-a-difference.wf",
        rejected: br#"fn need(v: &[u64]) -> result: u64 reads(v.len) contract {
  requires v^.len <= 2_u64;
} {
  return v^.len;
}

fn caller(start: u64, end: u64) -> result: u64 pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  let ordered = start <= end;
  let within = end <= 4_u64;
  if band(ordered, within) {
    let r = need(v: &values[start..end]);
    return r;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(start: 1_u64, end: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `values[start..end].len` is `end - start` [REF-4], so this call needs `end - start <= 2_u64`: where `start <= end` holds, bind the difference with the exact `-`, `let width = end - start;` (a `-wrap` difference carries no relation to the range's length), and guard the call with `if width <= 2_u64` where skipping it is the intended behavior; or bind the range with one preceding `let`, use the binding in the call, and establish the relation over the binding\n",
        ],
        repaired: &[
            br#"fn need(v: &[u64]) -> result: u64 reads(v.len) contract {
  requires v^.len <= 2_u64;
} {
  return v^.len;
}

fn caller(start: u64, end: u64) -> result: u64 pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  let ordered = start <= end;
  let within = end <= 4_u64;
  if band(ordered, within) {
    let width = end - start;
    if width <= 2_u64 {
      let r = need(v: &values[start..end]);
      return r;
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(start: 1_u64, end: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn need(v: &[u64]) -> result: u64 reads(v.len) contract {
  requires v^.len <= 2_u64;
} {
  return v^.len;
}

fn caller(start: u64, end: u64) -> result: u64 pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  let ordered = start <= end;
  let within = end <= 4_u64;
  if band(ordered, within) {
    let part = &values[start..end];
    if part^.len <= 2_u64 {
      let r = need(v: part);
      return r;
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(start: 1_u64, end: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // Span's fields are ENT-2(a) terms. The length is their exact
        // difference, which need not first copy each endpoint into a binding.
        name: "call-requirement-over-a-range-with-field-endpoints.wf",
        rejected: br#"struct Span {
  start: u64;
  end: u64;
}

fn need(v: &[u64]) -> result: u64 reads(v.len) contract {
  requires v^.len <= 2_u64;
} {
  return v^.len;
}

fn caller(span: Span) -> result: u64 pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  let ordered = span.start <= span.end;
  let within = span.end <= 4_u64;
  if band(ordered, within) {
    let r = need(v: &values[span.start..span.end]);
    return r;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let span = Span(start: 1_u64, end: 3_u64);
  let r = caller(span: span);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `values[span.start..span.end].len` is `span.end - span.start` [REF-4], so this call needs `span.end - span.start <= 2_u64`: where `span.start <= span.end` holds, bind the difference with the exact `-`, `let width = span.end - span.start;` (a `-wrap` difference carries no relation to the range's length), and guard the call with `if width <= 2_u64` where skipping it is the intended behavior; or bind the range with one preceding `let`, use the binding in the call, and establish the relation over the binding\n",
        ],
        repaired: &[
            br#"struct Span {
  start: u64;
  end: u64;
}

fn need(v: &[u64]) -> result: u64 reads(v.len) contract {
  requires v^.len <= 2_u64;
} {
  return v^.len;
}

fn caller(span: Span) -> result: u64 pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  let ordered = span.start <= span.end;
  let within = span.end <= 4_u64;
  if band(ordered, within) {
    let width = span.end - span.start;
    if width <= 2_u64 {
      let r = need(v: &values[span.start..span.end]);
      return r;
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let span = Span(start: 1_u64, end: 3_u64);
  let r = caller(span: span);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"struct Span {
  start: u64;
  end: u64;
}

fn need(v: &[u64]) -> result: u64 reads(v.len) contract {
  requires v^.len <= 2_u64;
} {
  return v^.len;
}

fn caller(span: Span) -> result: u64 pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  let ordered = span.start <= span.end;
  let within = span.end <= 4_u64;
  if band(ordered, within) {
    let part = &values[span.start..span.end];
    if part^.len <= 2_u64 {
      let r = need(v: part);
      return r;
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let span = Span(start: 1_u64, end: 3_u64);
  let r = caller(span: span);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "call-requirement-equating-range-length-differences.wf",
        rejected:
            br#"fn pair(left: &[u64], right: &[u64]) -> result: u64 reads(left.len) contract {
  requires left^.len == right^.len;
} {
  return left^.len;
}

fn caller(s: u64, e: u64, t: u64, u: u64) -> result: u64 pure {
  let a = array_filled::<u64, 4>(value: 0_u64);
  let b = array_filled::<u64, 4>(value: 0_u64);
  let ordered = s <= e;
  let within = e <= 4_u64;
  let other_ordered = t <= u;
  let other_within = u <= 4_u64;
  let first = band(ordered, within);
  let second = band(other_ordered, other_within);
  if band(first, second) {
    let r = pair(left: &a[s..e], right: &b[t..u]);
    return r;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(s: 1_u64, e: 3_u64, t: 0_u64, u: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `a[s..e].len` is `e - s` and `b[t..u].len` is `u - t` [REF-4], so this call needs `e - s == u - t`, and an equality over the difference of two distinct endpoints has no difference-bound form [ENT-4]: where the start is at most the end, bind each such difference with the exact `-`, as `let width = e - s;` (a `-wrap` difference carries no relation to the range's length), bind its range with one preceding `let`, as `let part = &a[s..e];`, and pass `&part^[0_u64..width]` in its place, whose length is `width` itself; then establish the requirement over those bindings\n",
        ],
        repaired: &[
            br#"fn pair(left: &[u64], right: &[u64]) -> result: u64 reads(left.len) contract {
  requires left^.len == right^.len;
} {
  return left^.len;
}

fn caller(s: u64, e: u64, t: u64, u: u64) -> result: u64 pure {
  let a = array_filled::<u64, 4>(value: 0_u64);
  let b = array_filled::<u64, 4>(value: 0_u64);
  let ordered = s <= e;
  let within = e <= 4_u64;
  let other_ordered = t <= u;
  let other_within = u <= 4_u64;
  let first = band(ordered, within);
  let second = band(other_ordered, other_within);
  if band(first, second) {
    let width = e - s;
    let other_width = u - t;
    let part = &a[s..e];
    let other = &b[t..u];
    if width == other_width {
      let r = pair(left: &part^[0_u64..width], right: &other^[0_u64..other_width]);
      return r;
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(s: 1_u64, e: 3_u64, t: 0_u64, u: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
];

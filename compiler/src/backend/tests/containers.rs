//! Allocation identity and retained aggregate boundaries. Whole-value
//! container behavior lives in the programs corpus; these observers add native
//! ownership evidence an exit code cannot provide.

use super::{compile_link_and_run, emit};

fn observe_allocations(module: &str) -> String {
    for declaration in ["declare ptr @malloc(i64)", "declare void @free(ptr)"] {
        assert_eq!(
            module.lines().filter(|line| *line == declaration).count(),
            1
        );
    }
    for symbol in [
        "wf_observe_allocate",
        "wf_observe_release",
        "wf_fixture_main",
    ] {
        assert!(!module.contains(symbol));
    }
    let result = module
        .replace("@malloc(", "@wf_observe_allocate(")
        .replace("@free(", "@wf_observe_release(")
        .replace("@main(", "@wf_fixture_main(");
    assert_eq!(
        result
            .replace("@wf_observe_allocate(", "@malloc(")
            .replace("@wf_observe_release(", "@free(")
            .replace("@wf_fixture_main(", "@main("),
        module
    );
    result
}

/// Retired subject: the allocation-refusal sweep, which [STOR-8]'s total
/// allocation leaves without a source-visible subject; successor: this test,
/// which keeps the owner-identity half the observer still observes, including
/// the `Full(offered:)` owner return the sweep used to check.
///
/// The counts are derived from the fixture, not read back from it. `main`
/// runs `exercise` for the sixteen seeds `0_u64..16_u64`; each `exercise`
/// makes twelve `allocate_put` calls and each of those one
/// `box_new::<u64>(value: v)`, and [STOR-1] gives a `Box<T>` "one
/// compiler-derived allocation released by one compiler-derived free at owner
/// scope exit [STOR-3]" while [TYPE-9] stores its content "in exactly one heap
/// object the `Box` value owns". So 16 x 12 = 192 owners and 192 frees, and
/// the twelfth of each seed is the owner offered to a genuinely full table and
/// handed straight back as `Full(offered: back)`: 16 of those. An open
/// `allocations=` would let an owner go missing without this line changing.
#[test]
fn owning_map_collision_and_tombstone_preserve_every_owner_identity() {
    let source = include_bytes!("../../../../tests/programs/containers/owning-map.wf");
    let module = observe_allocations(&emit(source));
    let output = compile_link_and_run(&module, Some(include_str!("owning_map_observer.c")), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let report = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        report,
        "PASS status=0 allocations=192 releases=192 full_original_owners=16 live=0\n"
    );
}

fn check_migration(source: &[u8], behavior: bool) {
    let mut module = observe_allocations(&emit(source));
    let body = "define i32 @wf__main_body(";
    assert_eq!(module.matches(body).count(), 1);
    module = module.replacen(body, "define i32 @wf_fixture_body(", 1);
    module.push_str("\ndeclare i32 @wf__main_body(i32, ptr)\n");
    let host = format!(
        "{}\n{}",
        if behavior {
            "#define BEHAVIOR_DEMOS"
        } else {
            ""
        },
        include_str!("owning_growth_observer.c")
    );
    let output = compile_link_and_run(&module, Some(&host), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let report = String::from_utf8(output.stdout).unwrap();
    let mut expected = String::from(
        "owning-growth: 304 matched executions; 912 resource releases; 592 backing releases\n",
    );
    if behavior {
        expected.push_str(
            "behavior: 3 stateful, branded-key and hostile-equality executions; every owner and release checked\n",
        );
    }
    assert_eq!(report, expected);
}

/// STOR-8 makes an allocation request total: host exhaustion terminates inside
/// the trusted base instead of returning a source-visible refusal. The retired
/// observer injected `malloc == NULL` and compared return code 70, which no
/// longer corresponds to a Whitefoot execution. This keeps every successful
/// migration budget, blocked/full domain result, exact allocation extent,
/// owner identity, and release comparison against the independent C control.
#[test]
fn direct_migration_budgets_and_domain_limits_preserve_state_and_allocation_identity() {
    check_migration(
        include_bytes!("../../../../tests/programs/containers/owning-growth.wf"),
        false,
    );
}

#[test]
fn generic_migration_and_behavior_limits_preserve_state_and_allocation_identity() {
    check_migration(
        include_bytes!("../../../../tests/programs/containers/owning-behavior.wf"),
        true,
    );
}

fn aggregate_transfer_cost(body: &str) -> (u64, u64) {
    let mut copy_bytes = 0;
    let mut vector_bytes = 0;
    for line in body.lines() {
        if line.contains("call ")
            && (line.contains("@llvm.memcpy.") || line.contains("@llvm.memmove."))
        {
            let length = line
                .rsplit_once(", i64 ")
                .expect("constant transfer size")
                .1;
            copy_bytes += length.split(',').next().unwrap().parse::<u64>().unwrap();
        }
        if let Some((_, vector)) = line.split_once(" = load <") {
            let (count, element) = vector.split_once(" x i").expect("integer vector");
            let bits = element.split('>').next().unwrap().parse::<u64>().unwrap();
            vector_bytes += count.parse::<u64>().unwrap() * bits / 8;
        }
    }
    (copy_bytes, vector_bytes)
}

#[test]
fn retained_priority_helpers_do_not_copy_the_inline_run() {
    for source in [
        include_bytes!("../../../../tests/programs/containers/priority.wf").as_slice(),
        include_bytes!("../../../../tests/conformance/cases/run-generic-priority-behavior.wf")
            .as_slice(),
    ] {
        let module = emit(source);
        let mut names = Vec::new();
        let retained = module
            .lines()
            .map(|line| {
                if line.starts_with("define ")
                    && let Some((_, symbol)) = line.split_once('@')
                    && let Some((name, _)) = symbol.split_once('(')
                {
                    let name = name.trim_matches('"');
                    let plain = name.strip_prefix("wf_").unwrap_or(name);
                    // A register-returned helper's internal body belongs to
                    // its public entry, the retained helper, and is inlined
                    // into it (compiler/src/backend/abi.rs), so it is
                    // neither retained nor counted.
                    if !plain.ends_with(".body")
                        && ["push", "pop"].iter().any(|prefix| {
                            plain == *prefix || plain.starts_with(&format!("{prefix}$instance$"))
                        })
                    {
                        names.push(name.to_owned());
                        return line.strip_suffix(" {").unwrap().to_owned() + " noinline {\n";
                    }
                }
                line.to_owned() + "\n"
            })
            .collect::<String>();
        assert!(matches!(names.len(), 2 | 4), "{names:?}");
        let optimized = super::host_optimized_module(&retained);
        for name in names {
            let header = optimized
                .lines()
                .find(|line| {
                    line.starts_with("define ")
                        && (line.contains(&format!("@{name}("))
                            || line.contains(&format!("@\"{name}\"(")))
                })
                .unwrap_or_else(|| panic!("missing retained {name}"));
            let start = optimized.find(header).unwrap();
            let end = start + optimized[start..].find("\n}").unwrap() + 2;
            let body = &optimized[start..end];
            assert_eq!(aggregate_transfer_cost(body), (0, 0), "{name}: {body}");
            for allocator in ["@malloc(", "@calloc(", "@realloc("] {
                assert!(!body.contains(allocator), "{name}: {body}");
            }
            assert!(
                optimized.lines().any(|line| line.contains("call ")
                    && (line.contains(&format!("@{name}("))
                        || line.contains(&format!("@\"{name}\"(")))),
                "retained {name} is actually called"
            );
        }
    }
}

#[test]
fn aggregate_transfer_inspector_counts_copy_and_vector_traffic() {
    assert_eq!(
        aggregate_transfer_cost(
            "call void @llvm.memcpy.p0.p0.i64(ptr %a, ptr %b, i64 144, i1 false)\n%v = load <4 x i64>, ptr %c\n"
        ),
        (144, 32)
    );
}

/// The retired 512-byte optimized-transfer ceiling depended on Clang forwarding
/// the caller's load snapshot; Apple Clang 15 may retain that extra transfer.
/// Check the compiler-owned guarantees instead: complete aligned independent
/// storage, and a take that captures its element address before updating the
/// descriptor and transferring the element. Native execution checks the values;
/// optimized transfer counts remain an experiment result, not a portable gate.
#[test]
fn taking_then_swapping_keeps_independent_storage_and_orders_the_take() {
    let source = br#"nocopy struct Row {
  words: Array<u64, 32>;
}

fn accept(seen: &u64, value: Row) -> result: unit writes(seen) {
  for (index in 0_u64..32_u64) {
    set deref(seen) = deref(seen) +wrap value.words[index];
  }
  return unit;
}

fn transfer(values: &Slots<Row, 4>, index: u64, seen: &u64) -> result: unit writes(values), writes(seen) contract {
  requires index + 2_u64 <= deref(values).len;
  ensures deref(values).len + 1_u64 == deref(entry(values)).len;
} {
  let value = take_back(window: values);
  swap(first: &deref(values)[index], second: &value);
  accept(seen: seen, value: move value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<Row, 4>();
  for @fill (
    index in 0_u64..4_u64,
    invariant length_lo: values.len >= index,
    invariant length_hi: values.len <= index
  ) {
    let next = index + 1_u64;
    let value = next * 10_u64;
    let words = array_filled::<u64, 32>(value: value);
    let row = Row(words: words);
    place_back(window: &values, value: move row);
  }
  let seen = 0_u64;
  transfer(values: &values, index: 0_u64, seen: &seen);
  if seen != 320_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  transfer(values: &values, index: 1_u64, seen: &seen);
  if seen != 960_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if values[0_u64].words[0_u64] != 40_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if values[1_u64].words[31_u64] != 30_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  let next = take_back(window: &values);
  accept(seen: &seen, value: move next);
  let last = take_back(window: &values);
  accept(seen: &seen, value: move last);
  if seen != 3200_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = emit(source);
    let transfer = super::emitted_function(&module, "transfer");
    let taken = transfer
        .lines()
        .find(|line| line.contains("call void @wf_take_back$"))
        .and_then(|line| line.split_once("(ptr "))
        .and_then(|(_, actuals)| actuals.split_once(", ptr "))
        .map(|(destination, _)| destination)
        .expect("take result destination");
    let accepted = transfer
        .lines()
        .find(|line| line.contains("@wf_accept("))
        .and_then(|line| line.rsplit_once(", ptr "))
        .and_then(|(_, actual)| actual.strip_suffix(')'))
        .expect("owned callback argument");
    assert_ne!(taken, accepted, "distinct storage groups: {transfer}");
    for pointer in [taken, accepted] {
        let allocation = transfer
            .lines()
            .find_map(|line| {
                line.trim_start()
                    .strip_prefix(&format!("{pointer} = alloca "))
            })
            .unwrap_or_else(|| panic!("{pointer} must have independent storage: {transfer}"));
        let (ty, alignment) = allocation.rsplit_once(", align ").expect("aligned root");
        assert_eq!(alignment, "8", "natural Row alignment: {transfer}");
        assert!(
            module
                .lines()
                .any(|line| line == format!("{ty} = type {{ [32 x i64] }}")),
            "each allocation must contain a complete Row: {transfer}"
        );
    }

    let take = super::emitted_prelude_row(&module, "take_back");
    let instructions = take.lines().map(str::trim_start).collect::<Vec<_>>();
    let (element_transfer, copy) = instructions
        .iter()
        .enumerate()
        .find(|(_, line)| {
            line.contains("call void @llvm.memmove.") || line.contains("call void @llvm.memcpy.")
        })
        .expect("nonzero element transfer");
    let element = copy
        .split_once(", ptr ")
        .and_then(|(_, source)| source.split_once(','))
        .map(|(pointer, _)| pointer)
        .expect("captured element source");
    let capture = instructions
        .iter()
        .position(|line| line.starts_with(&format!("{element} = getelementptr inbounds ")))
        .expect("capture the physical element address");
    let descriptor_update = instructions
        .iter()
        .position(|line| line.starts_with("store i64 "))
        .expect("update the window length");
    assert!(
        capture < descriptor_update && descriptor_update < element_transfer,
        "capture the old address, update the descriptor, then transfer: {take}"
    );

    let retained = module
        .lines()
        .map(|line| {
            if line.starts_with("define ")
                && (line.contains("@wf_transfer(") || line.contains("@wf_accept("))
            {
                line.strip_suffix(" {").unwrap().to_owned() + " noinline {\n"
            } else {
                line.to_owned() + "\n"
            }
        })
        .collect::<String>();
    let output = super::compile_and_run(&retained);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn growing_a_run_keeps_the_original_data_at_capacity_and_target_limits() {
    let source = include_bytes!("../../../../tests/programs/growable_vec.wf");
    let mut module = observe_allocations(&emit(source));
    // The retired region/store actual was the leading pointer in v0.59.
    // v0.60's one heap leaves exactly the four declared scalar parameters.
    assert!(module.contains("define i64 @wf_growth_trace(i8 %v0, i64 %v1, i64 %v2, i64 %v3)"));
    module = module.replacen(
        "define i32 @wf__main_body(",
        "define i32 @wf_fixture_body(",
        1,
    );
    module.push_str("\ndeclare i32 @wf__main_body(i32, ptr)\n");
    let output = compile_link_and_run(&module, Some(include_str!("growth_observer.c")), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"growth: 36 boundary traces; all owners returned\n"
    );
}

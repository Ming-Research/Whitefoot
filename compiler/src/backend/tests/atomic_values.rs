//! The atomic-value operation is emitted directly without a lock or operation
//! call. Atomic-only statements retain the ordinary waiting scheduling pass.
//! Execution correctness is covered by the program witness.
use super::*;

const OPERATIONS: &[u8] =
    br#"fn ticket(counter: &Shared<AtomicValue>) -> result: Option<u64> reads(counter) waits {
  let number = None<u64>();
  atomic state = &counter^ {
    set number = atomic_add(cell: state, value: 1_u64);
  }
  return number;
}

fn observe(counter: &Shared<AtomicValue>) -> result: u64 reads(counter) waits {
  let number = 0_u64;
  atomic state = &counter^ {
    set number = atomic_load(cell: state);
  }
  return number;
}

fn raise_peak(counter: &Shared<AtomicValue>) -> result: u64 reads(counter) waits {
  let number = 0_u64;
  atomic state = &counter^ {
    set number = atomic_raise(cell: state, value: 12_u64);
  }
  return number;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let counter = shared_atomic_new(value: 0_u64);
  let first = ticket(counter: &counter);
  let before = raise_peak(counter: &counter);
  let seen = observe(counter: &counter);
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn single_atomic_value_operations_emit_direct_hardware_operations_without_locks() {
    let module = compile(OPERATIONS);
    for name in ["ticket", "observe", "raise_peak"] {
        let body = emitted_body(&module, name);
        assert_eq!(
            body.matches("call i32 @wf__context_pass(").count(),
            1,
            "one existing scheduling pass per atomic-only statement: {body}"
        );
        assert!(
            body.contains("wf.atomic.pass.")
                && body.contains("@llvm.coro.save")
                && body.contains("@llvm.coro.suspend"),
            "the pass preserves the saved-token suspension protocol: {body}"
        );
        let pass = body.find("call i32 @wf__context_pass(").unwrap();
        let operation = body
            .find("load atomic i64")
            .or_else(|| body.find("atomicrmw add"))
            .unwrap();
        assert!(
            pass < operation,
            "the pass must precede the hardware operation: {body}"
        );
        for forbidden in [
            "@wf__shared_acquire",
            "@wf__shared_take",
            "@wf__shared_unlock",
            "@wf__shared_share",
            "@wf__shared_release",
            "@wf_atomic_",
        ] {
            assert!(!body.contains(forbidden), "{name} has {forbidden}: {body}");
        }
    }
    let ticket = emitted_body(&module, "ticket");
    assert_eq!(ticket.matches("atomicrmw add").count(), 1, "{ticket}");
    assert!(!ticket.contains("cmpxchg"), "add must not retry: {ticket}");
    assert!(
        ticket.contains("store atomic i64 9223372036854775808"),
        "full additions restore the saturation cap: {ticket}"
    );
    let observe = emitted_body(&module, "observe");
    assert_eq!(observe.matches("load atomic i64").count(), 1, "{observe}");
    assert!(!observe.contains("atomicrmw"), "{observe}");
    let peak = emitted_body(&module, "raise_peak");
    assert_eq!(peak.matches("atomicrmw umax").count(), 1, "{peak}");
    assert!(
        peak.contains("load atomic i64"),
        "a stopped maximum is read-only: {peak}"
    );
    assert!(
        peak.contains("select i1") && peak.contains("9223372036854775808"),
        "the prior maximum clamps transient saturation overshoot: {peak}"
    );
}

/// This composition-isolation check does not claim a pre-amendment baseline:
/// an exact old/new compiler comparison remains an external CI comparison.
#[test]
fn adding_an_atomic_value_user_does_not_change_an_ordinary_statement_body() {
    let source = include_str!("../../../../tests/programs/shared_statement_ir.wf");
    let extra = r#"
fn atomic_observer(cell: &Shared<AtomicValue>) -> result: u64 reads(cell) waits {
  let value = 0_u64;
  atomic state = &cell^ {
    set value = atomic_load(cell: state);
  }
  return value;
}
"#;
    let before = compile(source.as_bytes());
    let after = compile(format!("{source}{extra}").as_bytes());
    assert!(
        !emitted_body(&after, "ordinary").contains("call i32 @wf__context_pass("),
        "ordinary acquisition already supplies its scheduling boundary"
    );
    assert_eq!(
        emitted_body(&before, "ordinary"),
        emitted_body(&after, "ordinary")
    );
}

/// WAIT-2 requires a ready peer to progress even when atomic loads answer at
/// once. The atomic-only statement reuses the existing scheduling pass before
/// its targets, so polling cannot indefinitely retain a ready peer's driver.
#[test]
fn atomic_only_polling_keeps_the_waiting_boundary_progress_check() {
    let module = compile(include_bytes!(
        "../../../../tests/programs/atomic_value_progress.wf"
    ));
    let body = emitted_body(&module, "atomic_poll");
    assert!(
        body.contains("@wf__context_pass"),
        "an atomic-only loop must yield to a ready same-driver producer: {body}"
    );
}

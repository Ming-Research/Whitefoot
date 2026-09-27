#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

fn input<'input>(path: &'input str, bytes: &'input [u8]) -> SourceInput<'input> {
    SourceInput::new(path, bytes)
}

fn make_bundle(inputs: &[SourceInput<'_>]) -> SourceBundle {
    SourceBundle::with_limits(inputs, SourceLimits::REPRESENTABLE).unwrap()
}

#[test]
fn caller_order_defines_source_ids() {
    let bundle = make_bundle(&[input("z-last.wf", b"z"), input("a-first.wf", b"a")]);
    let observed: Vec<_> = bundle
        .iter()
        .map(|(id, file)| (id.ordinal(), file.logical_path().as_str()))
        .collect();
    assert_eq!(observed, [(0, "z-last.wf"), (1, "a-first.wf")]);
}

#[test]
fn identical_bytes_at_distinct_paths_remain_distinct() {
    let bundle = make_bundle(&[input("one.wf", b"same"), input("two.wf", b"same")]);
    assert_eq!(bundle.len(), 2);
    assert_eq!(
        bundle.file(SourceId::from_ordinal(0)).unwrap().bytes(),
        b"same"
    );
    assert_eq!(
        bundle.file(SourceId::from_ordinal(1)).unwrap().bytes(),
        b"same"
    );
}

#[test]
fn duplicate_path_reports_both_positions() {
    let error = SourceBundle::with_limits(
        &[
            input("same.wf", b"first"),
            input("middle.wf", b"middle"),
            input("same.wf", b"second"),
        ],
        SourceLimits::REPRESENTABLE,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        SourceBundleError::DuplicateLogicalPath {
            first_position: 0,
            duplicate_position: 2,
            ..
        }
    ));
}

#[test]
fn earliest_duplicate_in_transport_order_wins_across_path_groups() {
    let error = SourceBundle::with_limits(
        &[
            input("a", b"first-a"),
            input("z", b"first-z"),
            input("z", b"second-z"),
            input("a", b"second-a"),
        ],
        SourceLimits::REPRESENTABLE,
    )
    .unwrap_err();
    assert!(matches!(
        &error,
        SourceBundleError::DuplicateLogicalPath { .. }
    ));
    if let SourceBundleError::DuplicateLogicalPath {
        path,
        first_position,
        duplicate_position,
    } = error
    {
        assert_eq!(path.as_str(), "z");
        assert_eq!(first_position, 1);
        assert_eq!(duplicate_position, 2);
    }
}

#[test]
fn arbitrary_source_bytes_survive_ingestion() {
    let bytes = vec![0xff, 0x00, b'\r', b'\n', 0x80];
    let bundle = make_bundle(&[SourceInput::new("raw.wf", &bytes)]);
    assert_eq!(
        bundle.file(SourceId::from_ordinal(0)).unwrap().bytes(),
        bytes
    );
}

#[test]
fn impossible_reservation_is_an_explicit_storage_failure() {
    let mut values = Vec::<u8>::new();
    assert_eq!(
        try_reserve_exact(&mut values, usize::MAX, SourceLimit::SourceBytes, u64::MAX,),
        Err(SourceBundleError::StorageUnavailable {
            limit: SourceLimit::SourceBytes,
            requested: u64::MAX,
        })
    );
}

#[test]
fn logical_paths_are_closed_and_portable() {
    for path in [
        "",
        "/absolute.wf",
        "a//b.wf",
        "./a.wf",
        "a/../b.wf",
        "a\\b.wf",
        "snowman-\u{2603}.wf",
        "line\nbreak.wf",
    ] {
        assert!(LogicalPath::parse(path).is_err(), "accepted {path:?}");
    }
    assert!(LogicalPath::parse("dir-1/name_2.wf").is_ok());
}

#[test]
fn explicit_limits_accept_edges_and_reject_each_input_category() {
    let exact = SourceLimits {
        max_sources: 1,
        max_logical_path_bytes: 8,
        max_source_bytes: 3,
        max_total_source_bytes: 3,
        max_binding_bytes: 128,
    };
    assert!(SourceBundle::with_limits(&[input("edge.wf", b"abc")], exact).is_ok());
    let cases = [
        SourceBundle::with_limits(&[input("one.wf", b"a"), input("two.wf", b"b")], exact),
        SourceBundle::with_limits(&[input("too-long.wf", b"")], exact),
        SourceBundle::with_limits(&[input("edge.wf", b"abcd")], exact),
        SourceBundle::with_limits(
            &[input("one.wf", b"ab"), input("two.wf", b"cd")],
            SourceLimits {
                max_sources: 2,
                max_logical_path_bytes: 8,
                max_source_bytes: 3,
                max_total_source_bytes: 3,
                max_binding_bytes: 128,
            },
        ),
    ];
    let expected = [
        SourceLimit::Sources,
        SourceLimit::LogicalPathBytes,
        SourceLimit::SourceBytes,
        SourceLimit::TotalSourceBytes,
    ];
    for (result, expected_limit) in cases.into_iter().zip(expected) {
        assert!(matches!(
            result,
            Err(SourceBundleError::LimitExceeded { limit, .. }) if limit == expected_limit
        ));
    }
}

#[test]
fn path_limit_precedes_path_syntax_work() {
    let limits = SourceLimits {
        max_sources: 1,
        max_logical_path_bytes: 1,
        max_source_bytes: 1,
        max_total_source_bytes: 1,
        max_binding_bytes: 128,
    };
    assert!(matches!(
        SourceBundle::with_limits(&[input("a\\b", b"")], limits),
        Err(SourceBundleError::LimitExceeded {
            limit: SourceLimit::LogicalPathBytes,
            ..
        })
    ));
}

#[test]
fn record_limits_precede_path_validation_and_owned_copy() {
    let limits = SourceLimits {
        max_sources: 1,
        max_logical_path_bytes: 8,
        max_source_bytes: 1,
        max_total_source_bytes: 1,
        max_binding_bytes: 128,
    };
    assert!(matches!(
        SourceBundle::with_limits(&[input("a\\b", b"xx")], limits),
        Err(SourceBundleError::LimitExceeded {
            limit: SourceLimit::SourceBytes,
            maximum: 1,
            actual: 2,
        })
    ));
}

/// A span is validated against its bundle's source and read through a
/// bundle: equal coordinates read each bundle's own bytes.
#[test]
fn spans_are_half_open_and_read_through_their_bundle() {
    let bundle = make_bundle(&[input("span.wf", b"abcd")]);
    let source = SourceId::from_ordinal(0);
    let middle = bundle
        .span(source, ByteOffset::new(1), ByteOffset::new(3))
        .unwrap();
    assert_eq!(bundle.span_bytes(middle), Some(&b"bc"[..]));
    assert_eq!(
        bundle
            .file(middle.source())
            .map(|file| file.logical_path().as_str()),
        Some("span.wf")
    );

    let eof = bundle
        .span(source, ByteOffset::new(4), ByteOffset::new(4))
        .unwrap();
    assert_eq!(bundle.span_bytes(eof), Some(&b""[..]));
    assert!(matches!(
        bundle.span(source, ByteOffset::new(3), ByteOffset::new(2)),
        Err(SpanError::Reversed { .. })
    ));
    assert!(matches!(
        bundle.span(source, ByteOffset::new(0), ByteOffset::new(5)),
        Err(SpanError::OutOfBounds { .. })
    ));
    assert!(matches!(
        bundle.span(
            SourceId::from_ordinal(1),
            ByteOffset::new(0),
            ByteOffset::new(0)
        ),
        Err(SpanError::UnknownSource(_))
    ));

    let same_length = make_bundle(&[input("other.wf", b"WXYZ")]);
    let other_middle = same_length
        .span(source, ByteOffset::new(1), ByteOffset::new(3))
        .unwrap();
    assert_eq!(bundle.span_bytes(middle), Some(&b"bc"[..]));
    assert_eq!(same_length.span_bytes(other_middle), Some(&b"XY"[..]));
    assert_eq!(same_length.span_bytes(middle), Some(&b"XY"[..]));
}

#[test]
fn compilation_inputs_are_nonempty_before_prelude_injection() {
    for limits in [
        SourceLimits::REPRESENTABLE,
        SourceLimits {
            max_sources: 0,
            ..SourceLimits::REPRESENTABLE
        },
    ] {
        assert_eq!(
            SourceBundle::with_prelude(&[], limits),
            Err(SourceBundleError::EmptySourceSequence)
        );
    }

    // The lower-level transport remains usable by envelope-checking tools;
    // the parser already rejects its empty form as an invocation failure.
    assert!(SourceBundle::with_limits(&[], SourceLimits::REPRESENTABLE).is_ok());
}

#[test]
fn prelude_injection_keeps_the_complete_bundle_source_ceiling() {
    let inputs = [input("empty.wf", b"")];
    let prelude_count = u32::try_from(crate::prelude::DECLARATIONS.len()).unwrap();
    let exact = SourceLimits {
        max_sources: prelude_count + 1,
        ..SourceLimits::REPRESENTABLE
    };
    let bundle = SourceBundle::with_prelude(&inputs, exact).unwrap();
    let first = bundle.file(SourceId::from_ordinal(0)).unwrap();
    assert_eq!(first.logical_path().as_str(), "empty.wf");
    assert!(first.bytes().is_empty());
    assert!(first.prelude().is_none());
    assert_eq!(bundle.len(), usize::try_from(prelude_count).unwrap() + 1);

    assert_eq!(
        SourceBundle::with_prelude(
            &inputs,
            SourceLimits {
                max_sources: prelude_count,
                ..exact
            },
        ),
        Err(SourceBundleError::LimitExceeded {
            limit: SourceLimit::Sources,
            maximum: u64::from(prelude_count),
            actual: u64::from(prelude_count) + 1,
        })
    );
}

#[test]
fn supplied_prelude_paths_do_not_reserve_writer_logical_paths() {
    let inputs = [input("prelude/Array.wf", b"writer bytes")];
    let bundle = SourceBundle::with_prelude(&inputs, SourceLimits::REPRESENTABLE).unwrap();
    assert_eq!(
        bundle.file(SourceId::from_ordinal(0)).unwrap().bytes(),
        b"writer bytes"
    );
    assert_eq!(
        bundle
            .files()
            .iter()
            .filter(|file| file.logical_path().as_str() == "prelude/Array.wf")
            .count(),
        2
    );
    assert!(bundle.files()[0].prelude().is_none());
    assert!(
        bundle
            .files()
            .iter()
            .skip(1)
            .all(|file| file.prelude().is_some())
    );
    let duplicated = [inputs[0], inputs[0]];
    assert!(matches!(
        SourceBundle::with_prelude(&duplicated, SourceLimits::REPRESENTABLE),
        Err(SourceBundleError::DuplicateLogicalPath {
            first_position: 0,
            duplicate_position: 1,
            ..
        })
    ));
}

//! Wire encoders follow RFC 1951 sections 3.2.4-7. Expectations are explicit
//! plaintext, error variants, or an independent flat overlap expansion;
//! neither a research decoder nor the WF decoder produces expected bytes.

use super::support::{build_program_with_driver, compile_programs};

const LENGTH_BASE: [usize; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_BITS: [u32; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DISTANCE_BASE: [usize; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DISTANCE_BITS: [u32; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const TRUNCATED: u64 = u64::MAX;
const STORED: u64 = u64::MAX - 1;
const TREE: u64 = u64::MAX - 2;
const CODE: u64 = u64::MAX - 3;
const DISTANCE: u64 = u64::MAX - 4;
const BLOCK: u64 = u64::MAX - 5;
const FULL: u64 = u64::MAX - 6;

#[derive(Default)]
struct Bits {
    bytes: Vec<u8>,
    used: usize,
}
impl Bits {
    fn bits(&mut self, value: u32, width: u32) {
        for at in 0..width {
            if self.used.is_multiple_of(8) {
                self.bytes.push(0);
            }
            let last = self.bytes.len() - 1;
            self.bytes[last] |= (((value >> at) & 1) as u8) << (self.used % 8);
            self.used += 1;
        }
    }
    fn code(&mut self, value: u32, width: u32) {
        for at in (0..width).rev() {
            self.bits((value >> at) & 1, 1);
        }
    }
    fn fixed(&mut self, final_block: bool) {
        self.bits(u32::from(final_block), 1);
        self.bits(1, 2);
    }
    fn symbol(&mut self, symbol: u32) {
        match symbol {
            0..=143 => self.code(symbol + 48, 8),
            144..=255 => self.code(symbol - 144 + 400, 9),
            256..=279 => self.code(symbol - 256, 7),
            280..=287 => self.code(symbol - 280 + 192, 8),
            _ => panic!("not a fixed Huffman symbol"),
        }
    }
    fn stored(&mut self, payload: &[u8], final_block: bool) {
        assert!(payload.len() <= 65_535);
        self.bits(u32::from(final_block), 1);
        self.bits(0, 2);
        while !self.used.is_multiple_of(8) {
            self.bits(0, 1);
        }
        self.bits(payload.len() as u32, 16);
        self.bits((payload.len() as u32) ^ 65_535, 16);
        for &byte in payload {
            self.bits(u32::from(byte), 8);
        }
    }
}
struct Case {
    name: String,
    wire: Vec<u8>,
    capacity: u32,
    expected: Result<Vec<u8>, u64>,
}
fn good(name: &str, wire: Vec<u8>, expected: Vec<u8>) -> Case {
    Case {
        name: name.to_owned(),
        wire,
        capacity: expected.len() as u32,
        expected: Ok(expected),
    }
}
fn bad(name: &str, wire: Vec<u8>, capacity: u32, error: u64) -> Case {
    Case {
        name: name.to_owned(),
        wire,
        capacity,
        expected: Err(error),
    }
}
fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut wire = Bits::default();
    wire.fixed(true);
    for byte in 0..256 {
        wire.symbol(byte);
    }
    wire.symbol(256);
    cases.push(good("every literal", wire.bytes, (0..=255).collect()));

    let mut wire = Bits::default();
    wire.fixed(true);
    wire.symbol(65);
    let mut plain = vec![65];
    for (index, (&base, &bits)) in LENGTH_BASE.iter().zip(&LENGTH_BITS).enumerate() {
        let high = if index == 27 { 30 } else { (1 << bits) - 1 };
        for delta in [0, high].into_iter().take(if high == 0 { 1 } else { 2 }) {
            wire.symbol(index as u32 + 257);
            wire.bits(delta, bits);
            wire.code(0, 5);
            plain.extend(std::iter::repeat_n(65, base + delta as usize));
        }
    }
    wire.symbol(256);
    cases.push(good("every length endpoint", wire.bytes, plain));

    let mut plain = (0..32768).map(|at| (at * 29 + 7) as u8).collect::<Vec<_>>();
    let mut wire = Bits::default();
    wire.stored(&plain, false);
    wire.fixed(true);
    for (index, (&base, &bits)) in DISTANCE_BASE.iter().zip(&DISTANCE_BITS).enumerate() {
        let high = (1 << bits) - 1;
        for delta in [0, high].into_iter().take(if high == 0 { 1 } else { 2 }) {
            wire.symbol(257);
            wire.code(index as u32, 5);
            wire.bits(delta, bits);
            for _ in 0..3 {
                plain.push(plain[plain.len() - base - delta as usize]);
            }
        }
    }
    wire.symbol(256);
    cases.push(good(
        "every distance endpoint and mixed blocks",
        wire.bytes,
        plain,
    ));
    let plain = (0..65535).map(|at| (at * 17 + 3) as u8).collect::<Vec<_>>();
    let mut wire = Bits::default();
    wire.stored(&plain, true);
    cases.push(good("maximum stored block", wire.bytes, plain));
    cases.push(good("empty stored block", vec![1, 0, 0, 255, 255], vec![]));
    cases.push(good("padding", vec![3, 252], vec![]));
    cases.push(good(
        "trailing bytes",
        vec![3, 0, 222, 173, 190, 239],
        vec![],
    ));

    let mut overlap = Bits::default();
    overlap.fixed(true);
    overlap.symbol(65);
    overlap.symbol(285);
    overlap.code(0, 5);
    overlap.symbol(66);
    overlap.symbol(256);
    let mut plain = vec![65; 259];
    plain.push(66);
    for capacity in [0, 1, 2, 258, 259, 260, 300] {
        if capacity < 260 {
            cases.push(bad(
                "overlap output bound",
                overlap.bytes.clone(),
                capacity,
                FULL,
            ));
        } else {
            cases.push(Case {
                name: "overlap output complete".into(),
                wire: overlap.bytes.clone(),
                capacity,
                expected: Ok(plain.clone()),
            });
        }
    }
    for capacity in 0..=3 {
        let wire = vec![1, 3, 0, 252, 255, 65, 66, 67];
        if capacity < 3 {
            cases.push(bad("stored output bound", wire, capacity, FULL));
        } else {
            cases.push(good("stored exact output bound", wire, b"ABC".to_vec()));
        }
    }
    cases.push(bad("reserved block", vec![7], 0, BLOCK));
    cases.push(bad("stored complement", vec![1, 1, 0, 0, 0, 65], 0, STORED));
    cases.push(bad(
        "stored truncated payload",
        vec![1, 5, 0, 250, 255, 104, 101, 108, 108],
        10,
        TRUNCATED,
    ));
    for hlit in [30, 31] {
        cases.push(bad(
            "reserved literal count",
            vec![5 | (hlit << 3)],
            0,
            TREE,
        ));
    }
    for symbol in [286, 287] {
        let mut wire = Bits::default();
        wire.fixed(true);
        wire.symbol(symbol);
        cases.push(bad("reserved literal symbol", wire.bytes, 10, CODE));
    }
    for symbol in [30, 31] {
        let mut wire = Bits::default();
        wire.fixed(true);
        wire.symbol(65);
        wire.symbol(257);
        wire.code(symbol, 5);
        cases.push(bad("reserved distance symbol", wire.bytes, 10, DISTANCE));
    }
    let mut wire = Bits::default();
    wire.fixed(true);
    wire.symbol(257);
    wire.code(0, 5);
    wire.symbol(256);
    cases.push(bad("distance before history", wire.bytes, 10, DISTANCE));
    cases.extend(dynamic_cases());
    cases
}

#[test]
fn decoder_wire_boundaries_run_as_one_batch_through_the_actual_wf_decoder() {
    let mut llvm = compile_programs(&[
        "raw_deflate.wf",
        "raw_deflate_dynamic.wf",
        "raw_deflate_dynamic_decode.wf",
        "raw_deflate_probe.wf",
    ]);
    for name in ["main", "wf__main_body"] {
        let definition = format!("define i32 @{name}(");
        assert_eq!(llvm.matches(&definition).count(), 1);
        llvm = llvm.replacen(&definition, &format!("define i32 @fixture_{name}("), 1);
    }
    llvm.push_str("\ndeclare i32 @wf__main_body(i32, ptr)\n");
    // Load WF's exact range-reference pairs in LLVM; C passes pointers to
    // the pointer-plus-count pair and makes no assumption about the platform's
    // aggregate coercions. A `&[u8]` parameter is that pair: the address of the
    // first element of the range and the element count, which is its one
    // measure [REF-4, MSR-1]. It crosses the call as those two arguments.
    llvm.push_str(
        r#"
define i64 @wf_test_decode(ptr %source, ptr %destination) {
  %input = load { ptr, i64 }, ptr %source
  %input.data = extractvalue { ptr, i64 } %input, 0
  %input.len = extractvalue { ptr, i64 } %input, 1
  %output = load { ptr, i64 }, ptr %destination
  %output.data = extractvalue { ptr, i64 } %output, 0
  %output.len = extractvalue { ptr, i64 } %output, 1
  %result = call i64 @wf_decode_case(ptr %input.data, i64 %input.len, ptr %output.data, i64 %output.len)
  ret i64 %result
}
"#,
    );
    let program = build_program_with_driver(&llvm, Some(include_str!("raw_deflate_probe.c")));
    let cases = cases();
    let mut input = Vec::new();
    for case in &cases {
        input.extend_from_slice(&(case.wire.len() as u32).to_le_bytes());
        input.extend_from_slice(&case.capacity.to_le_bytes());
        input.extend_from_slice(&case.wire);
    }
    let output = program.run_with_piped_input(&input, true);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let mut remainder = output.stdout.as_slice();
    for case in &cases {
        assert!(remainder.len() >= 8, "missing result for {}", case.name);
        let result = u64::from_le_bytes(remainder[..8].try_into().unwrap());
        remainder = &remainder[8..];
        match &case.expected {
            Ok(bytes) => {
                assert_eq!(result, bytes.len() as u64, "{}", case.name);
                assert!(
                    remainder.len() >= bytes.len(),
                    "truncated output: {}",
                    case.name
                );
                assert_eq!(&remainder[..bytes.len()], bytes, "{}", case.name);
                remainder = &remainder[bytes.len()..];
            }
            Err(error) => assert_eq!(result, *error, "{}", case.name),
        }
    }
    assert!(remainder.is_empty(), "extra decoder output");
    eprintln!(
        "decoder: {} independent wire/capacity cases in one image and one process",
        cases.len()
    );
}

fn dynamic_cases() -> Vec<Case> {
    let mut cases = vec![
        good(
            "dynamic repeats and no distance",
            vec![
                5, 192, 201, 8, 0, 0, 0, 0, 176, 238, 191, 210, 186, 255, 254, 48, 11, 1,
            ],
            vec![65, 65, 65],
        ),
        good(
            "dynamic single distance",
            vec![13, 192, 129, 8, 0, 0, 0, 192, 48, 182, 249, 75, 253, 179, 0],
            vec![65, 65, 65, 65],
        ),
        good(
            "dynamic two empty distances",
            vec![
                5, 193, 1, 4, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 0,
            ],
            vec![],
        ),
        bad(
            "dynamic missing EOB",
            vec![5, 192, 129, 8, 0, 0, 0, 0, 32, 182, 253, 173, 0],
            20,
            TREE,
        ),
        bad("dynamic first repeat", vec![5, 0, 2, 36], 20, TREE),
        bad(
            "dynamic oversubscribed lengths",
            vec![5, 0, 146, 0],
            20,
            TREE,
        ),
    ];
    let complete = vec![
        5, 192, 201, 8, 0, 0, 0, 0, 176, 238, 191, 210, 186, 255, 254, 48, 11, 1,
    ];
    for end in 0..complete.len() {
        cases.push(bad(
            &format!("dynamic truncation at {end}"),
            complete[..end].to_vec(),
            20,
            TRUNCATED,
        ));
    }
    cases
}

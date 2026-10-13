use std::collections::BTreeMap;

use super::support::{build_program, compile_program};

#[derive(Debug, PartialEq, Eq)]
struct Export {
    cut: u64,
    entries: BTreeMap<Vec<u8>, u64>,
}

struct Bytes<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Bytes<'a> {
    fn take(&mut self, count: usize) -> &'a [u8] {
        let end = self.offset.checked_add(count).expect("bounded wire offset");
        let bytes = self.bytes.get(self.offset..end).expect("complete wire field");
        self.offset = end;
        bytes
    }

    fn byte(&mut self) -> u8 {
        self.take(1)[0]
    }

    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take(4).try_into().expect("four bytes"))
    }

    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.take(8).try_into().expect("eight bytes"))
    }
}

/// An independent SNAP-2 decoder for this witness's concrete u64 schema.
/// It accepts either map traversal order, checks every frame and cut, and
/// rejects duplicate keys, trailing bytes and a missing terminal.
fn decode(bytes: &[u8]) -> Export {
    let mut input = Bytes { bytes, offset: 0 };
    assert_eq!(input.take(8), b"WFSNAP01");
    let cut = input.u64();
    let mut entries = BTreeMap::new();
    loop {
        match input.byte() {
            0x01 => {
                let key_length = usize::try_from(input.u64()).expect("key fits host");
                assert_eq!(input.u64(), cut, "each record repeats the cut");
                let mut body = Vec::new();
                loop {
                    match input.byte() {
                        0x02 => {
                            let count = usize::try_from(input.u32()).expect("chunk fits host");
                            assert!((1..=4096).contains(&count), "SNAP-2 chunk bound");
                            body.extend_from_slice(input.take(count));
                        }
                        0x03 => break,
                        tag => panic!("unexpected chunk tag {tag:#x}"),
                    }
                }
                let mut body = Bytes { bytes: &body, offset: 0 };
                let key = body.take(key_length).to_vec();
                assert_eq!(body.byte(), 0x08, "u64 schema tag");
                let value = body.u64();
                assert_eq!(body.offset, body.bytes.len(), "exact record body");
                assert!(entries.insert(key, value).is_none(), "each key occurs once");
            }
            0xff => {
                assert_eq!(input.u64(), u64::try_from(entries.len()).expect("entry count"));
                assert_eq!(input.offset, input.bytes.len(), "terminal ends stream");
                return Export { cut, entries };
            }
            tag => panic!("unexpected record tag {tag:#x}"),
        }
    }
}

/// This oracle uses logical journal events, independently of the exporter,
/// its byte encoding and map traversal. The sequence-8 overwrite is present
/// in the journal but must not affect replay to the captured cut of seven.
fn replay(cut: u64) -> Export {
    let events = [(6, b'a', 11), (7, b'b', 22), (8, b'a', 99)];
    let mut entries = BTreeMap::new();
    for (sequence, key, value) in events {
        if sequence <= cut {
            entries.insert(vec![key], value);
        }
    }
    Export { cut, entries }
}

#[test]
fn capture_retains_the_cut_image_and_live_export_fails_the_replay_oracle() {
    let program = build_program(&compile_program("snapshot_export.wf"));
    let captured = program.run_with_workers_and_arguments(Some("1"), &[]);
    assert_eq!(captured.status.code(), Some(0), "{captured:?}");
    assert!(captured.stderr.is_empty(), "{captured:?}");
    assert_eq!(decode(&captured.stdout), replay(7));

    // This ordinary second capture reads the live table after a->99/seq8,
    // using a separate held cut of seven so metadata alone cannot detect it.
    // Its valid export must fail the exact same independent replay-7 oracle.
    // Comparing only keys or the repeated cut would miss the defect.
    let live = program.run_with_workers_and_arguments(Some("1"), &[b"live"]);
    assert_eq!(live.status.code(), Some(0), "{live:?}");
    assert!(live.stderr.is_empty(), "{live:?}");
    let live = decode(&live.stdout);
    let mut expected_live = replay(8);
    expected_live.cut = 7;
    assert_eq!(live, expected_live, "negative control reached the live state");
    assert_ne!(live, replay(7), "the replay oracle detects live-table export");
}

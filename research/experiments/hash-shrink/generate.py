#!/usr/bin/env python3
"""Generate one ordinary-module benchmark per representation; called by run.py."""
from pathlib import Path

HERE = Path(__file__).resolve().parent


def generate(shape, destination):
    key = "u64"
    make_key = "return index;"
    key_id = "return key^;"
    extra = ""
    if shape == "bytes":
        key = "ByteKey"
        extra = "nocopy struct ByteKey { bytes: Box<Array<u8, 8>>; }"
        make_key = """let bytes = array_filled::<u8, 8>(value: 0_u8);
  for (i in 0_u64..8_u64) {
    let amount = i * 8_u64;
    let shift = cvt.wrap::<u64, u32>(amount);
    let shifted = ishr(index, shift);
    let byte = cvt.wrap::<u64, u8>(shifted);
    set bytes[i] = byte;
  }
  let owned = box_new::<Array<u8, 8>>(value: bytes);
  return ByteKey(bytes: move owned);"""
        key_id = """let id = 0_u64;
  for (i in 0_u64..8_u64) {
    let amount = i * 8_u64;
    let shift = cvt.wrap::<u64, u32>(amount);
    let byte = cvt.wrap::<u8, u64>(key^.bytes.inner[i]);
    let shifted = ishl(byte, shift);
    set id = ior(id, shifted);
  }
  return id;"""
    value = "unit"
    make_value = "return unit;"
    inspect = "return key_id(key: key);"
    if shape == "wide":
        value = "WideValue"
        extra = "nocopy struct WideValue { words: Box<Array<u64, 32>>; }"
        make_value = """let words = array_filled::<u64, 32>(value: index);
  let owned = box_new::<Array<u64, 32>>(value: words);
  return WideValue(words: move owned);"""
        inspect = """let id = key_id(key: key);
  for (i in 0_u64..32_u64) {
    if value^.words.inner[i] != id { return 18446744073709551615_u64; }
  }
  return id;"""
    source = (HERE / "benchmark.wf.in").read_text()
    for token, replacement in {"@KEY@": key, "@VALUE@": value,
                               "@EXTRA@": extra, "@MAKE_KEY@": make_key,
                               "@KEY_ID@": key_id, "@MAKE_VALUE@": make_value,
                               "@INSPECT@": inspect}.items():
        source = source.replace(token, replacement)
    Path(destination).write_text(source)

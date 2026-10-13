#!/usr/bin/env python3
"""Emit pair.wf, the minimal example of the call-grain-args investigation.

Two adjacent calls of mix_words over two disjoint heap arrays are permitted to
overlap (PAR-1). Each call loops over its own range, carrying a state from word to word
so that the compiler cannot split the loop; each element runs a data
dependent number of rounds (4 to 11) of STEPS mixing steps. The compiler prices
that inner loop at the fixed 16 trips of its call grain, so the static work of
mix_words is about 16 * (units per element); `run.py calibrate` picks STEPS so
that this is about 2200, the figure of Snowghost's interning calls.

rotation() and multiplier() are mirrored in pair_reference.h; the C reference
and this generator derive every step from the same two formulas.
"""
import sys

LIMIT = 16777216


def rotation(k):
    return 1 + (k * 7) % 61


def multiplier(k):
    return ((6364136223846793005 * (k + 1) + 1442695040888963407) & ((1 << 63) - 1)) | 1


def source(steps):
    if steps < 1:
        raise ValueError("steps must be at least 1")
    body = []
    for k in range(steps):
        body.append(f"      let a{k} = irotl(value, {rotation(k)}_u32);")
        body.append(f"      let b{k} = a{k} *wrap {multiplier(k)}_u64;")
        body.append(f"      let c{k} = ixor(b{k}, r);")
        body.append(f"      set value = c{k} +wrap {k + 1}_u64;")
    steps_text = "\n".join(body)
    return f"""fn mix_words(words: &[u64], salt: u64) -> result: u64 writes(words) {{
  doc "Mixes every word of its own range in place and returns the final state. The state carries from word to word (each word is mixed with the state the previous word left), so the loop has no reassociable accumulator and is refused by the loop rule; only the two calls side by side can overlap. Each word runs four to eleven rounds, the count read from the word itself, so the rounds loop has no closed-form trip count.";
  let count = words^.len;
  let state = salt;
  for (i in 0_u64..count) {{
    let word = words^[i];
    let value = ixor(word, state);
    let low = iand(value, 7_u64);
    let rounds = 4_u64 +wrap low;
    let r = 0_u64;
    loop @spin {{
      if r == rounds {{
        break @spin;
      }}
{steps_text}
      set r = r +wrap 1_u64;
    }}
    set words^[i] = value;
    set state = value;
  }}
  return state;
}}

fn workload(repetitions: u64, extent: u64, seed: u64) -> result: u64 pure {{
  doc "Calls mix_words on two disjoint arrays, repetitions times. The two calls are adjacent statements with disjoint write paths, so PAR-1 permits them to overlap.";
  if extent > {LIMIT}_u64 {{
    return 0_u64;
  }}
  invariant bounded: extent <= {LIMIT}_u64;
  let left = box_array_filled::<u64>(count: extent, value: seed);
  let seed_right = seed +wrap 1_u64;
  let right = box_array_filled::<u64>(count: extent, value: seed_right);
  let i = 0_u64;
  let total = seed;
  loop @repeat {{
    if i == repetitions {{
      break @repeat;
    }}
    let n = left.inner.len;
    let m = right.inner.len;
    let window_left = &left.inner[0_u64..n];
    let window_right = &right.inner[0_u64..m];
    let salt_right = i +wrap seed;
    let a = mix_words(words: window_left, salt: i);
    let b = mix_words(words: window_right, salt: salt_right);
    let both = ixor(a, b);
    set total = total +wrap both;
    set i = i +wrap 1_u64;
  }}
  return total;
}}
"""


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("usage: generate.py STEPS OUTPUT.wf")
    with open(sys.argv[2], "w") as output:
        output.write(source(int(sys.argv[1])))

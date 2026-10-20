#!/usr/bin/env python3
"""Write the frozen stage-one grid; no compiler or third-party dependency."""
import argparse
import csv
import hashlib
import random
from pathlib import Path

SEED = 0x574650415231
SHAPES = ("flat", "balanced", "skew90", "skew99", "spine", "dag")
STEPS = (1, 10, 100, 1000, 10000, 100000, 1000000, 10000000)
SPREADS = (1, 100, 10000)
FIELDS = ("cell", "split", "family", "shape", "bound", "steps", "tasks",
          "calls", "spread", "words", "seed", "source", "sha256")


def grid():
    cells = []
    def add(family, shape, bound, steps, tasks, calls=1, spread=1, words=0):
        cells.append(dict(family=family, shape=shape, bound=bound, steps=steps,
                          tasks=tasks, calls=calls, spread=spread, words=words))
    for shape in SHAPES:
        tasks = 128 if shape.startswith("skew") else 64 if shape == "spine" else 16
        for steps in STEPS:
            for spread in SPREADS:
                add("work", shape, "arithmetic", steps, tasks, spread=spread)
        for calls in (10, 10000, 10000000):
            add("hot", shape, "arithmetic", 1, 4, calls=calls)
        for spread in SPREADS:
            add("bound", shape, "memory", 8388608, 4, spread=spread, words=8388608)
            add("bound", shape, "allocation", 1024, 128, spread=spread)
    sizes = {"flat": (1, 1000, 1000000, 100000000), "balanced": (1, 1024, 65536),
             "skew90": (1, 128, 1024), "skew99": (1, 128, 1024),
             "spine": (1, 128, 1024), "dag": (1, 128)}
    for shape, counts in sizes.items():
        for tasks in counts:
            add("size", shape, "arithmetic", 1, tasks)
    order = list(range(len(cells)))
    random.Random(SEED).shuffle(order)
    held = set(order[:len(order) // 2])
    for i, cell in enumerate(cells):
        cell.update(cell=f"p{i:03d}", split="held-out" if i in held else "visible",
                    seed=SEED)
    return cells


def source(c):
    # Three equally likely cost tiers on a log scale. The seed and index are
    # runtime WF data; the outer recurrence prevents hot calls being hoisted.
    middle = 10 if c["spread"] == 100 else 100 if c["spread"] == 10000 else 1
    text = f'''alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn suite_mix(value: u64) -> result: u64 pure {{
  let shifted = ishr.wrap(value, 13_u32);
  let mixed = ixor(value, shifted);
  let product = mixed *wrap 6364136223846793005_u64;
  return product +wrap 1442695040888963407_u64;
}}

fn suite_task(data: &[u64], seed: u64, index: u64) -> result: u64 reads(data) {{
  let tagged = index +wrap {SEED}_u64;
  let tag = suite_mix(value: tagged);
  let tier = tag % 3_u64;
  let divisor = if tier == 0_u64 {{
    give 1_u64;
  }} else if tier == 1_u64 {{
    give {middle}_u64;
  }} else {{
    give {c['spread']}_u64;
  }}
  let quotient = {c['steps']}_u64 / divisor;
  let work = if quotient == 0_u64 {{
    give 1_u64;
  }} else {{
    give quotient;
  }}
  let value = seed +wrap index;
'''
    if c["bound"] == "arithmetic":
        text += '''  for (step in 0_u64..work) {
    set value = suite_mix(value: value);
  }
'''
    elif c["bound"] == "memory":
        text += '''  for (at in 0_u64..work) {
    if at < data^.len {
      set value = value +wrap data^[at];
    }
  }
'''
    else:
        text += '''  let cell = box_array_filled::<u64>(count: work, value: value);
  for (at in 0_u64..cell.inner.len) {
    let item = value +wrap at;
    set cell.inner[at] = suite_mix(value: item);
  }
  for (at in 0_u64..cell.inner.len) {
    set value = value +wrap cell.inner[at];
  }
'''
    text += "  return value;\n}\n\n"
    shape = c["shape"]
    text += '''fn suite_site(data: &[u64], seed: u64, first: u64, count: u64) -> result: u64 reads(data) {
'''
    if shape == "flat":
        text += '''  let sum = 0_u64;
  for (at in 0_u64..count) {
    let index = first +wrap at;
    let part = suite_task(data: data, seed: seed, index: index);
    set sum = sum +wrap part;
  }
  return sum;
'''
    elif shape == "dag":
        # Each diamond has shared predecessors (a,b -> c,d -> e); diamonds
        # are chained to keep their bounded DAG inside sequential code.
        text += '''  let result = seed;
  for (at in 0_u64..count) {
    let origin = first +wrap at;
    let second = origin +wrap 1_u64;
    let third = origin +wrap 2_u64;
    let fourth = origin +wrap 3_u64;
    let fifth = origin +wrap 4_u64;
    let a = suite_task(data: data, seed: result, index: origin);
    let b = suite_task(data: data, seed: result, index: second);
    let joined = a +wrap b;
    let c = suite_task(data: data, seed: joined, index: third);
    let d = suite_task(data: data, seed: joined, index: fourth);
    let next = c +wrap d;
    let e = suite_task(data: data, seed: next, index: fifth);
    set result = e;
  }
  return result;
'''
    else:
        text += '''  if count == 0_u64 {
    return 0_u64;
  }
  if count == 1_u64 {
    return suite_task(data: data, seed: seed, index: first);
  }
'''
        divisor = {"balanced": 2, "skew90": 10, "skew99": 100}.get(shape)
        if divisor:
            text += f'''  let fraction = count / {divisor}_u64;
  let left_count = if fraction == 0_u64 {{
    give 1_u64;
  }} else {{
    give fraction;
  }}
  let right_count = count - left_count;
  let second = first +wrap left_count;
  let left = suite_site(data: data, seed: seed, first: first, count: left_count);
  let right = suite_site(data: data, seed: seed, first: second, count: right_count);
'''
        else:
            text += '''  let remaining = count - 1_u64;
  let second = first +wrap 1_u64;
  let left = suite_task(data: data, seed: seed, index: first);
  let right = suite_site(data: data, seed: seed, first: second, count: remaining);
'''
        text += "  return left +wrap right;\n"
    text += f'''}}

fn suite_entry() -> result: u64 pure {{
  let data = box_array_filled::<u64>(count: {c['words']}_u64, value: 0_u64);
  for (at in 0_u64..data.inner.len) {{
    let initial = at +wrap {SEED}_u64;
    set data.inner[at] = suite_mix(value: initial);
  }}
  let checksum = {SEED}_u64;
  for (call in 0_u64..{c['calls']}_u64) {{
    let next = suite_site(data: &data.inner[0_u64..data.inner.len], seed: checksum, first: 0_u64, count: {c['tasks']}_u64);
    set checksum = next;
  }}
  return checksum;
}}

fn main() -> status: ExitStatus pure {{
  let checksum = suite_entry();
  if checksum == 0_u64 {{
    return exit_status(code: 1_u8);
  }}
  return exit_status(code: 0_u8);
}}
'''
    if c["bound"] != "memory":
        text = text.replace("reads(data)", "pure")
    return text


def generate(directory):
    directory.mkdir(parents=True, exist_ok=False)
    cells = grid()
    for cell in cells:
        content = source(cell)
        cell["source"] = cell["cell"] + ".wf"
        cell["sha256"] = hashlib.sha256(content.encode()).hexdigest()
        (directory / cell["source"]).write_text(content)
    with (directory / "manifest.tsv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, FIELDS, delimiter="\t")
        writer.writeheader()
        writer.writerows(cells)
    return cells


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="fresh scratch directory")
    args = parser.parse_args()
    print(f"wrote {len(generate(args.output))} cells to {args.output}")

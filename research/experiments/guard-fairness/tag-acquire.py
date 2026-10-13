#!/usr/bin/env python3
"""CI research observer for one emitted acquire site; never changes timing IR.

The native runtime cannot distinguish the probe's checkout and return calls.
This diagnostic-only transform selects a caller/ordinal supplied by build.sh,
and marks the first entry into its retry block using a generic runtime API.
"""

import re
import sys
from pathlib import Path


MARKER = "wf__guard_measure_next_acquire"
ACQUIRE = re.compile(r"^\s+%\S+ = call i32 @wf__shared_acquire\([^\n]+\)$", re.M)


def transform(text, function, ordinal, expected_calls, after_call):
    if MARKER in text:
        raise ValueError("IR already contains the diagnostic observer")
    functions = list(re.finditer(r"^define [^\n]*@" + re.escape(function) + r"\([^\n]*\)[^\n]*\{\n(.*?)^\}", text, re.M | re.S))
    if len(functions) != 1:
        raise ValueError("observer caller missing or ambiguous")
    match = functions[0]
    body = match.group(1)
    calls = list(ACQUIRE.finditer(body))
    if len(calls) != expected_calls or not 2 <= ordinal <= expected_calls:
        raise ValueError("observer acquire layout changed")
    selected = calls[ordinal - 1]
    anchors = list(re.finditer(r"@" + re.escape(after_call) + r"\(", body))
    if len(anchors) != 1 or not calls[ordinal - 2].end() < anchors[0].start() < selected.start():
        raise ValueError("observer preceding call missing or moved")
    labels = list(re.finditer(r"^([-a-zA-Z$._0-9]+):[^\n]*$", body[:selected.start()], re.M))
    if not labels:
        raise ValueError("observer acquire block missing")
    label = labels[-1].group(1)
    branches = list(re.finditer(r"^([ \t]*)br label %" + re.escape(label) + r"[ \t]*$", body, re.M))
    if len(branches) != 1 or not anchors[0].end() < branches[0].start() < labels[-1].start():
        raise ValueError("observer first-entry predecessor missing or ambiguous")
    branch = branches[0]
    changed = body[:branch.start()] + branch.group(1) + "call void @" + MARKER + "()\n" + body[branch.start():]
    result = text[:match.start(1)] + changed + text[match.end(1):]
    result += "\ndeclare void @" + MARKER + "()\n"
    # The checkout, return retry calls, and later lazy-lock calls are intact.
    if ACQUIRE.findall(result) != ACQUIRE.findall(text):
        raise ValueError("observer changed acquire calls")
    return result


def self_test():
    text = """define ptr @worker(ptr %h) {
entry:
  %a = call i32 @wf__shared_acquire(ptr %s, i32 1, ptr %h)
  %work = call i64 @hold(i64 1)
return:
  br label %retry
retry:
  %b = call i32 @wf__shared_acquire(ptr %s, i32 1, ptr %h)
  switch i8 %again, label %done [ i8 1, label %retry ]
done:
  %c = call i32 @wf__shared_acquire(ptr %s, i32 1, ptr %h)
  ret ptr %h
}
"""
    result = transform(text, "worker", 2, 3, "hold")
    assert "call void @" + MARKER + "()\n  br label %retry" in result
    assert result.count("call void @" + MARKER) == 1
    for wrong in (text.replace("@hold", "@other"),
                  text.replace("  br label %retry", "  br label %done"),
                  text.replace("done:\n", "extra:\n  br label %retry\ndone:\n"),
                  text.replace("@worker", "@other"), result):
        try:
            transform(wrong, "worker", 2, 3, "hold")
        except ValueError:
            continue
        raise ValueError("ambiguous or missing observer anchor escaped")
    print("diagnostic acquire observer sensitivity fixtures passed")


if __name__ == "__main__":
    try:
        if sys.argv[1:] == ["self-test"]:
            self_test()
        else:
            source, output, function, ordinal, expected, after = sys.argv[1:]
            Path(output).write_text(transform(Path(source).read_text(), function, int(ordinal), int(expected), after))
    except (ValueError, OSError) as error:
        sys.exit(f"invalid diagnostic observer: {error}")

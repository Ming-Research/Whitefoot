#!/usr/bin/env python3
"""Where the regression stencil's two grids are mapped, by startup order.

A model of the mappings a launch makes on the hosted runners (Linux 6.17,
glibc 2.39), for research/investigations/data-placement. The kernel gives a
mapping the highest hole that holds it, and starts an anonymous mapping whose
size is a multiple of 2 MiB on a 2 MiB boundary inside a hole 2 MiB larger.
glibc gives a thread's first allocation a heap: 128 MiB mapped, trimmed to
the 64 MiB that starts on a 64 MiB boundary. A block of 32 MiB is mapped by
itself.

usage: layout.py            the grids' distance for every position of the C
                            library within 64 MiB, per startup order
       layout.py LIBC ORDER one launch: the C library's base (hexadecimal)
                            and an order, printing the grids' addresses
       layout.py --pads LIBC ORDER
                            the distance with an idle mapping of k times
                            2 MiB made first, k from 0 to 31
"""

import sys

MIB = 1 << 20
PAGE = 4096
GRID = 32 * MIB + PAGE
HEAP = 64 * MIB
ENTRY_STACK = 1024 * MIB + PAGE

# What is mapped before the oracle's reference and a timed call's two grids.
# "thread" is an idle thread with its own heap, settled before the next step.
ORDERS = {
    "entry": ["entry", "heap"],
    "thread-then-entry": ["thread", "entry", "heap"],
    "entry-one-heap": ["entry"],
    "thread-then-entry-one-heap": ["thread-no-heap", "entry"],
}


class Space:
    """The mappings below the C library."""

    def __init__(self, top):
        self.top = top
        self.maps = []

    def holes(self):
        edge = self.top
        for start, end in sorted(self.maps, reverse=True):
            if end < edge:
                yield end, edge
            edge = min(edge, start)
        yield 0, edge

    def map(self, size, aligned=False):
        need = size + (2 * MIB if aligned else 0)
        for low, high in self.holes():
            if high - low >= need:
                start = high - need
                if aligned:
                    start = (start + 2 * MIB - 1) & ~(2 * MIB - 1)
                self.maps.append((start, start + size))
                return start
        raise RuntimeError("no hole")

    def unmap(self, start, size):
        kept = []
        for s, e in self.maps:
            if e <= start or s >= start + size:
                kept.append((s, e))
                continue
            if s < start:
                kept.append((s, start))
            if e > start + size:
                kept.append((start + size, e))
        self.maps = kept


def thread_heap(space):
    mapped = space.map(2 * HEAP, aligned=True)
    heap = (mapped + HEAP - 1) & ~(HEAP - 1)
    if heap > mapped:
        space.unmap(mapped, heap - mapped)
    space.unmap(heap + HEAP, HEAP - (heap - mapped))


def launch(libc, order, pad=0, thread_stack=16 * MIB + PAGE):
    """The addresses of a timed call's first and second grid."""
    space = Space(libc)
    if pad:
        space.map(pad, aligned=pad % (2 * MIB) == 0)
    for step in ORDERS[order]:
        if step == "thread":
            space.map(thread_stack)
            thread_heap(space)
        elif step == "thread-no-heap":
            space.map(thread_stack)
        elif step == "entry":
            space.map(ENTRY_STACK)
        elif step == "heap":
            thread_heap(space)
    # The oracle's reference maps three grids and frees the first two.
    first, second = space.map(GRID), space.map(GRID)
    space.map(GRID)
    space.unmap(first, GRID)
    space.unmap(second, GRID)
    return space.map(GRID), space.map(GRID)


def main(arguments):
    if not arguments:
        for order in ORDERS:
            shares = {}
            for k in range(32):
                a, b = launch(0x7F0000000000 + k * 2 * MIB, order)
                shares[a - b] = shares.get(a - b, 0) + 1
            print(order + ": " + ", ".join(
                f"{distance:+#x} in {count}" for distance, count in
                sorted(shares.items(), key=lambda item: (-item[1], item[0]))))
    elif arguments[0] == "--pads":
        libc, order = int(arguments[1], 16), arguments[2]
        for k in range(32):
            a, b = launch(libc, order, pad=k * 2 * MIB)
            print(f"k={k} {a - b:+#x}")
    else:
        a, b = launch(int(arguments[0], 16), arguments[1])
        print(f"A={a:#x} B={b:#x} A-B={a - b:+#x}")


if __name__ == "__main__":
    main(sys.argv[1:])

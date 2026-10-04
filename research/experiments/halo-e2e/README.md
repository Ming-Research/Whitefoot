# Halo embedding comparison

This explicitly invoked experiment runs the unchanged Halo oracle scripts through
`pkg::embed` and an in-memory Whitefoot Redis test host. It compares typed RESP2
JSON with the existing Redis 7.0.15 observations; it never regenerates them.
The runner owns fixture transport, JSON formatting and comparison, not Lua
execution. The Whitefoot host owns commands and both reply conversions.

The embedding module, test program, runner and result record serve VM.md section
6 and its first end-to-end comparison. They live in the existing Halo library
and experiment homes, and are removed or superseded when the production firn
binding replaces this test host or Halo is retired.

No compiler, VM, heap or oracle files are changed by this experiment. Unsupported
commands and library functions are reported as failures rather than silently
removed from the corpus. See RESULTS.md for the measured coverage and gaps.

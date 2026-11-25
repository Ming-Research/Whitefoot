# Formatting command

The compiler's explicit request that rewrites sources into their canonical FORM-2 layout, and how it shares the layout derivation with the FORM-2 judgment.

### decision format-request-rewrites-in-place
`whitefootc --format`, over the sources `--check` takes or a module graph, replaces every source whose bytes differ from its FORM-2 rendering and writes nothing when any input fails.
- rejected: an agent proposed formatting during `--check` or compilation; the owner rejected it, because FORM-1 makes non-canonical input a hard error so that the bytes a writer keeps are the bytes the compiler judged.
- rejected: an agent proposed a formatter script in Whitefoot-kit or in each downstream project; the owner rejected it, because it would reimplement the FORM-2 rendering outside the compiler and drift from it.
- rejected: an agent proposed writing the canonical bytes to standard output; the owner rejected it, because a module program has many sources and the writer's next step is always to keep the canonical bytes.

### decision format-shares-the-layout-derivation
The bytes `--format` writes come from one library function that stops after grammar derivation and finalization, and the FORM-2 judgment shares its layout derivation in a walk of its own.
- rejected: an agent proposed a formatting path with a layout derivation of its own; the owner rejected it, because two layout derivations could drift apart.
- rejected: an agent proposed formatting only accepted programs; the owner rejected it, because a formatter that ran semantic checking would refuse a program whose remaining faults are semantic.
- rejected: an agent proposed making the FORM-2 judgment compare the rendering with the input bytes; the owner rejected it, because rewriting the judgment walk could move rejection positions the conformance cases pin, a risk larger than its gain.
- rejected: an agent proposed judging the written bytes by FORM-2 again before writing them; the owner rejected it, because a test already shows the formatted output passes the layout check, so it only repeats that and adds run time.

### decision graph-formatting-reports-as-check-does
Graph formatting forms the module program as `--check --graph` does, and reports any failure other than a source's own lexical or grammar rejection by rerunning module loading on the original bytes.
- rejected: an agent proposed a second package walk that tolerates graph rule failures; the owner rejected it, because the writer should see the diagnostic `--check` gives, not a diagnostic placed in a rendering that was never written.

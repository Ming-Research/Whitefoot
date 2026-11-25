# Typed syntax

How the compiler's stages read the parser's owned syntax tree.

### decision stages-read-shared-typed-syntax-views
Resolution, checking, module-graph reading and the driver read shared typed views of the parser's owned syntax, each production alternative decoded once.
- replaced: Each stage probing raw children and terminal offsets on its own was on main; the owner changed it, because that duplicated reading of the grammar made a grammar change reach every semantic consumer and let their readings disagree.
- rejected: an agent proposed copying the syntax into a separately allocated normalized tree; the owner rejected it, because the owned syntax already gives identity and source locations, while another tree needs identity translation and duplicate storage with no current consumer.
- rejected: an agent proposed moving semantic judgments into the syntax views; the owner rejected it, because grammar alternatives describe written form and cannot decide rules that depend on resolved declarations, checked types or proof facts.

### decision syntax-paths-and-declaration-graphs-built-once
A syntax view builds node paths only when asked, and the declaration reads of one resolved unit share one immutable adjacency and set of module roots.
- replaced: Building every node path up front, and each module's declaration graph anew, was on main; the owner changed it, because Halo's profile showed that setup repeated for consumers that use no paths or do not change the resolved input (data: `research/investigations/check-time/DESIGN.md`).

### decision syntax-views-index-descendants
On its first descendant query, a syntax view builds a preorder index of subtree intervals and per-production node lists, which later queries read.
- replaced: Walking the whole subtree on every descendant query was on main; the owner changed it, because the second Halo profile showed repeated full descendant walks (data: `research/investigations/check-time/DESIGN.md`).

# Termination of the `process_token` reprocess loop (Snowghost 1120edf)

Sources: `renderer/html/tree_builder/*.wf` at commit 1120edf, read with `git show`, no
repository file edited. All `file:line` below are per-file line numbers in that commit.
Model files (my transcription of the edges below, used only to check the measure against
the model): `graph.py`, `check.py`, `cycles2.py` in this directory. Nothing was executed on
the compiled tree builder; every trace below is by reading.

Result in one paragraph: the loop terminates for every token and every parser state. There
are 51 reprocess-return sites (edges). The token-independent mode graph has 13 elementary
cycles (not counting reset edges); every one of them is dead once the token class is fixed.
Two edge families are "reset" edges (E34 = EOF with a template open, E38 = `<table>` start
tag) and give further cycles; they terminate because a counter strictly decreases. The
measure is `(N_tmpl, R[class][mode], D)`, lexicographic. No cycle without a decrease was
found, so there is no non-terminating input to report. The per-token bound is a constant (6
reprocess iterations, 7 dispatches) except for the two reset families, whose bound is linear
in the open template count (EOF) and in the open-element stack depth (`<table>`).

## 0. How the loop works (dispatch.wf)

`process_token` (dispatch.wf:251-279) repeats: `should_dispatch_as_html` (dispatch.wf:91-158)
-> either `dispatch_html_content` (dispatch.wf:160-249, one `mode_*` per `state.mode`) or
`process_foreign_content` (foreign.wf:410-471). The loop ends at the first handler that
returns False. The token value never changes across iterations, but `state.mode`, the open
element stack, the template-mode stack, `original_mode` and a few flags do.

Facts used below (all read directly):

- The `InBody` start-tag handler `in_body_start_tag` (mode_body_start.wf:15-549) has 39
  `return Ok<Bool,...>` sites, each of the form `let rN = False()` (counted with grep:
  39 returns, 39 `= False()`). It never reprocesses; where it delegates to `mode_in_head`
  (l.38) it drops the result.
- `in_body_end_tag` (mode_body_end.wf:11-172) returns True only at l.35 (`</html>` with body
  in scope) and forwards `mode_in_head`'s result for `</template>` (l.16-17), which is
  always False. `</br>` (l.159-168) and `</p>` (l.84-95) return False, the fix in 417f91f
  is visible here (`</p>` with no p in button scope inserts a p and closes it; it does not
  reprocess).
- `mode_in_body` (mode_body_main.wf:37-91): Characters, Comment, Doctype are False; EOF
  forwards `mode_in_template` only when `template_modes` is non-empty (l.74-77); Start/End
  forward the two helpers above.
- Foreign content never returns True by itself (see F1-F4 below).
- Delegation results that are dropped (`let processed = ...; return False`) are not edges:
  mode_head3.wf:89,110; mode_head4.wf:31,66,75; mode_after.wf:54,109,128,204,211,252,259,272,309,316,323,335;
  mode_table.wf:155,220; mode_template.wf:39,99 (in template `mode_in_head`), and the like.
  Delegation results that ARE forwarded are listed in the reach column of section 1.

## 1. Edge table (51 True-return sites)

Counting convention: "51" = True-return call sites as seen from a mode handler. The helpers
`in_caption_close` (mode_caption_colgroup.wf:87) and `in_column_group_anything_else` (:226)
hold one `True()` literal each and are counted once per call site (edges 48/49 and 50/51);
edge 34 is one literal (mode_template.wf:114) counted once although 8 modes reach it by EOF
forwarding; edge 39 is one literal (mode_table.wf:346) reached from 5 token branches. The
checker expands edges to (edge, token class, from-mode, to-mode) tuples: 207 tuples
excluding edge 34, all satisfying the measure.

Legend: `-k` = pops k >= 1 elements, `+1` = pushes one element, `top:=` = replaces the top of
the template-mode stack (net depth 0). "RESET" = `reset_insertion_mode` (reset_mode.wf:18-149),
whose result is one of: InCell, InRow, InTableBody, InCaption, InColumnGroup, InTable,
the top template mode (InTemplate/InTable/InColumnGroup/InTableBody/InRow/InBody), InHead,
InBody, InFrameset, BeforeHead, AfterHead. Token: C = Characters, S = StartTag, E = EndTag.

| # | site | from-mode | token / guard | to-mode | open stack | template stack | other state |
|---|---|---|---|---|---|---|---|
| 1 | mode_initial.wf:86 | Initial | C not all-ws | BeforeHtml | 0 | 0 | quirks := Quirks (l.66-72) |
| 2 | mode_initial.wf:136 | Initial | S any | BeforeHtml | 0 | 0 | quirks := Quirks |
| 3 | mode_initial.wf:141 | Initial | E any | BeforeHtml | 0 | 0 | quirks := Quirks |
| 4 | mode_initial.wf:146 | Initial | EOF | BeforeHtml | 0 | 0 | quirks := Quirks |
| 5 | mode_simple.wf:109 | BeforeHtml | S name != html | BeforeHead | +1 (html) | 0 | before_html_anything_else l.146-167 |
| 6 | mode_simple.wf:116 | BeforeHtml | E head/body/html/br | BeforeHead | +1 | 0 | |
| 7 | mode_simple.wf:140 | BeforeHtml | EOF | BeforeHead | +1 | 0 | |
| 8 | mode_head.wf:53 | BeforeHead | S not html, not head | InHead | +1 (head) | 0 | head_pointer := new head |
| 9 | mode_head.wf:61 | BeforeHead | E head/body/html/br | InHead | +1 | 0 | head_pointer |
| 10 | mode_head.wf:88 | BeforeHead | EOF | InHead | +1 | 0 | head_pointer |
| 11 | mode_head2.wf:138 | InHead | S not in {html, base, basefont, bgsound, link, meta, title, noframes, style, noscript, script, template, head} | AfterHead | -1 (current node, `pop_open_if_any`) | 0 | in_head_anything_else l.179-184 |
| 12 | mode_head2.wf:165 | InHead | E body/html/br | AfterHead | -1 | 0 | |
| 13 | mode_head2.wf:173 | InHead | EOF | AfterHead | -1 | 0 | |
| 14 | mode_head4.wf:43 | InHeadNoscript | S not in {html, link, meta, style, basefont, bgsound, noframes, head, noscript} | InHead | -1 | 0 | l.87-92 |
| 15 | mode_head4.wf:57 | InHeadNoscript | E br | InHead | -1 | 0 | |
| 16 | mode_head4.wf:71 | InHeadNoscript | C not all ws/NUL | InHead | -1 | 0 | |
| 17 | mode_head4.wf:81 | InHeadNoscript | EOF | InHead | -1 | 0 | |
| 18 | mode_head4.wf:124 | Text | EOF | `original_mode` (any mode that started rcdata/rawtext/script: InHead, InHeadNoscript, InBody, table modes, InCell, InCaption, InTemplate, frameset modes) | -1 (l.122) | 0 | mode := original_mode (l.123) |
| 19 | mode_head3.wf:104 | AfterHead | S not html/body/frameset/head-like/head | InBody | +1 (body) | 0 | frameset_ok := True (l.135) |
| 20 | mode_head3.wf:117 | AfterHead | E body/html/br | InBody | +1 | 0 | frameset_ok := True |
| 21 | mode_head3.wf:125 | AfterHead | EOF | InBody | +1 | 0 | frameset_ok := True |
| 22 | mode_after.wf:27 | AfterBody | C not all ws | InBody | 0 | 0 | l.9-13 |
| 23 | mode_after.wf:59 | AfterBody | S name != html | InBody | 0 | 0 | |
| 24 | mode_after.wf:74 | AfterBody | E name != html | InBody | 0 | 0 | |
| 25 | mode_after.wf:264 | AfterAfterBody | S name != html | InBody | 0 | 0 | |
| 26 | mode_after.wf:277 | AfterAfterBody | C not all ws | InBody | 0 | 0 | |
| 27 | mode_after.wf:282 | AfterAfterBody | E any | InBody | 0 | 0 | |
| 28 | mode_body_end.wf:35 | InBody | E html, body in ordinary scope | AfterBody | 0 | 0 | reached only from InBody (End html is ignored in InCell, InCaption and the table-body/row/table lists) |
| 29 | mode_template.wf:57 | InTemplate | S caption/colgroup/tbody/tfoot/thead | InTable | 0 | top := InTable | l.52-56 |
| 30 | mode_template.wf:66 | InTemplate | S col | InColumnGroup | 0 | top := InColumnGroup | |
| 31 | mode_template.wf:75 | InTemplate | S tr | InTableBody | 0 | top := InTableBody | |
| 32 | mode_template.wf:86 | InTemplate | S td/th | InRow | 0 | top := InRow | |
| 33 | mode_template.wf:93 | InTemplate | S any other name that is not head-like (includes table, html) | InBody | 0 | top := InBody | l.82-92 |
| 34 | mode_template.wf:114 | InTemplate; also InBody, InTable, InTableBody, InRow, InCell, InCaption, InColumnGroup (via EOF forwarding, see below) | EOF and an html `template` is on the open stack | RESET | -k, k >= 1 (`pop_until_html_tag(template)`) plus implied-end-tag pops | -1 (`pop_template_mode`, no-op when empty) | AFE cleared to last marker; "E34" below |
| 35 | mode_table.wf:68 | InTable; InTableBody via :166; InRow via :284 (forwarded from `mode_in_table`) | C, current node is html table/tbody/template/tfoot/thead/tr | InTableText | 0 | 0 | original_mode := current mode (InTable/InTableBody/InRow) (l.66), pending_table_text cleared, pending_table_all_whitespace := True |
| 36 | mode_table.wf:108 | InTable | S col | InColumnGroup | -k (clear to table context) +1 (colgroup) | 0 | |
| 37 | mode_table.wf:133 | InTable | S td/th/tr | InTableBody | -k +1 (tbody) | 0 | |
| 38 | mode_table.wf:143 | InTable; InTableBody via :113; InRow via :220 (forwarded) | S table, table in table scope | RESET | -k, k >= 1 (pop_until table), no push | 0 | "E33" below |
| 39 | mode_table.wf:346 | InTableText | non-C: S :297, E :301, Comment :305, Doctype :309, EOF :313 (helper `in_table_text_flush` l.319-348) | original_mode, one of InTable/InTableBody/InRow | +j, j >= 0 (only for non-ws buffered text: `reconstruct_active_formatting_elements` under foster parenting pushes clones) | 0 | buffer cleared, foster_parenting restored to False |
| 40 | mode_table_body.wf:86 | InTableBody | S td/th | InRow | -k +1 (tr) | 0 | |
| 41 | mode_table_body.wf:107 | InTableBody | S caption/col/colgroup/tbody/tfoot/thead, some tbody/thead/tfoot in table scope | InTable | -k then -1 (>= 1 total) | 0 | |
| 42 | mode_table_body.wf:145 | InTableBody | E table, same guard | InTable | >= 1 pop | 0 | |
| 43 | mode_table_body.wf:214 | InRow | S caption/col/colgroup/tbody/tfoot/thead/tr, tr in table scope | InTableBody | >= 1 pop (tr) | 0 | |
| 44 | mode_table_body.wf:244 | InRow | E table, tr in table scope | InTableBody | >= 1 pop | 0 | |
| 45 | mode_table_body.wf:263 | InRow | E tbody/tfoot/thead, that name and tr both in table scope | InTableBody | >= 1 pop | 0 | |
| 46 | mode_table_body.wf:340 | InCell | E table/tbody/tfoot/thead/tr, that name in table scope | InRow | >= 1 pop (`in_cell_close` l.395-422 pops through td/th) | 0 | AFE cleared to marker |
| 47 | mode_table_body.wf:367 | InCell | S caption/col/colgroup/tbody/td/tfoot/th/thead/tr, td or th in table scope | InRow | >= 1 pop | 0 | AFE cleared to marker |
| 48 | mode_caption_colgroup.wf:22 | InCaption | E table, caption in table scope (`in_caption_close` l.78-92) | InTable | >= 1 pop (pop_until caption) | 0 | AFE cleared to marker |
| 49 | mode_caption_colgroup.wf:54 | InCaption | S caption/col/colgroup/tbody/td/tfoot/th/thead/tr, same guard | InTable | >= 1 pop | 0 | AFE cleared to marker |
| 50 | mode_caption_colgroup.wf:164 | InColumnGroup | S not html/col/template, current node is colgroup (`in_column_group_anything_else` l.209-231) | InTable | -1 (colgroup) | 0 | |
| 51 | mode_caption_colgroup.wf:200 | InColumnGroup | E not colgroup/col/template, current node is colgroup | InTable | -1 | 0 | |

Reach of E34 by EOF forwarding: InBody `mode_body_main.wf:74-77` (only if `template_modes`
non-empty), InTable `mode_table.wf:228`, InTableBody `mode_table_body.wf:178`, InRow `:296`,
InCell `:389`, InCaption `mode_caption_colgroup.wf:72`, InColumnGroup `:203`. The E34 guard
`stack_contains_html_tag(template)` (mode_template.wf:107) guarantees the stack holds a
template, so `pop_until_html_tag` (stack.wf:252-274) removes at least one.

Foreign-content sites (no True of their own):

- F1 foreign.wf:446: breakout start tag: `pop_out_of_foreign_content` (pops only non-HTML,
  non-integration-point elements; stack -k, k >= 0) then `dispatch_html_content` in
  `state.mode`; its result is returned. So F1 = "0 or more pops, then one of edges 1-51".
- F2 foreign.wf:461: end tag `br`/`p`: same as F1.
- F3 foreign.wf:407: any other end tag that reaches an HTML element: `dispatch_html_content`
  on the unmodified stack; result forwarded. Same shape.
- F4 foreign.wf:467: `EndOfFile() => True` in `process_foreign_content`. Unreachable: EOF
  always dispatches as HTML (dispatch.wf:93-97). See SUSPECT 1.

Recursive `process_token` calls (not loop iterations; each starts its own loop with a new
Characters token): mode_simple.wf:133 (BeforeHtml), mode_head.wf:80 (BeforeHead),
mode_caption_colgroup.wf:127 (InColumnGroup). Nesting depth: BeforeHtml -> BeforeHead ->
InHead (which handles characters inline, mode_head2.wf:22-45, returns False) = 3 frames;
the InColumnGroup call happens only after `in_column_group_anything_else` popped the colgroup
and set InTable, and Characters in InTable cannot reach InColumnGroup's handler again, so
1 extra frame. Each nested Characters token has at most 1 reprocess (section 3, class
Chars).

Handlers with no True return at all: InFrameset, AfterFrameset, AfterAfterFrameset,
InBody Characters/Comment/Doctype/StartTag, Text except EOF, every Comment/Doctype (except
InTableText, edge 39), the whole Doctype/Comment column of every mode.

## 2. Graph and cycles

Nodes: 21 modes (`InsertionMode` variants). Foreign dispatch adds no node: F1-F3 only pop
non-HTML elements and then run an ordinary edge on `state.mode`, and never
increase any measure component below (they never push, never touch template modes), so any
iteration sequence that mixes foreign and HTML dispatch is a path in the graph of edges 1-51.
Re-dispatch after an edge (foreign vs HTML is decided again from the stack) therefore cannot
create a cycle that the edge graph does not already contain.

### 2a. Token-independent cycles (mode graph, reset edges 34 and 38 removed): 13

All 13 are broken by the token, each on a different edge set, so none is a cycle for one token.

| cycle | edge going one way | edge going the other way | why not a cycle |
|---|---|---|---|
| InBody -> AfterBody -> InBody | 28 (E html) | 22, 23, 24 (C, S, E != html) | End html is consumed in AfterBody (mode_after.wf:63-72 sets AfterAfterBody, False) |
| InTable -> InColumnGroup -> InTable | 36 (S col) | 50, 51 (everything but col) | InColumnGroup consumes col (mode_caption_colgroup.wf:149-155) |
| InTable -> InTableBody -> InTable | 37 (S tr/td/th) | 41, 42 (S caption/col/colgroup/tbody/tfoot/thead, E table) | InTableBody consumes tr (l.69-77); InTable consumes tbody/thead/tfoot/caption/colgroup |
| InTableBody -> InRow -> InTableBody | 40 (S td/th) | 43, 44, 45 (S group/tr, E table/tbody/tfoot/thead) | InRow consumes td/th (mode_table_body.wf:191-199) |
| the 9 cycles through InTableText (InTable->InTableText->InTable, InTable->InTableBody->InRow->InTableText->InTable, InTableText->InRow->InTableBody->InTableText, and the rest of that family) | 35 (C only) | 39 (every non-C token) | InTableText consumes Characters (mode_table.wf:266-295) |

The 13 = 1 (InBody/AfterBody) + 1 (InTable/InColumnGroup) + 1 (InTable/InTableBody) + 1
(InTableBody/InRow) + 9 through InTableText; the enumeration is `python3 cycles2.py`.

### 2b. Reset cycles through edge 38 (S table): 13, one strongly connected component

SCC = {InTable, InTableBody, InRow, InColumnGroup}, class S table. Edges inside it: 38 (any
of the three table modes to a reset target inside the SCC) and 50 (InColumnGroup -> InTable).
Every traversal of either pops at least one element and pushes none:
38 pops the table (guard: table in table scope, `pop_until_html_tag` stops at the first html
table, so >= 1) and `reset_insertion_mode` does not touch the stack; 50 pops the colgroup.
So the open-element stack depth D strictly decreases around every cycle. The concrete
input `<table><caption><table><table>` uses edge 38 once: stack [html body table caption
table], mode InTable; `<table>` -> 38 pops the inner table, reset finds `caption` ->
InCaption; InCaption reprocesses `<table>` through `in_body_start_tag`, which inserts a
table (False). I did not construct an input that leaves 38 inside the SCC even once (the
reset target is the nearest stopper below the popped table, and a stopper of the SCC kind
below a table inserted by body rules needs a stack shape I could not build; template
barriers redirect it, e.g. `<template><div><table>` resets to the replaced template mode
InBody). That is reachability, not termination: the measure below does not need it.

### 2c. Reset cycles through edge 34 (EOF): many, all decreasing N_tmpl

Any EOF path that fires 34 twice (e.g. InBody -34-> InBody, InTemplate -34-> InTemplate)
is a cycle in the graph. Around such a cycle the number of html `template` elements on the
open stack, `N_tmpl`, drops by >= 1 at 34 and nothing raises it: the only code that
pushes a template element is the `<template>` branch of `mode_in_head` (mode_head2.wf:120-131),
which returns False, so it is on no edge. Edges 35-37 and 40 push tbody/tr/colgroup/tbody
only; edge 39 pushes formatting-element clones only.

Concrete trace (unbounded family, N nested templates, source `<template>` x N then EOF):
Initial -1-> BeforeHtml -5-> BeforeHead -8-> InHead, then each `<template>` is consumed in
InHead / InTemplate (mode_head2.wf:120-131 pushes template + template mode; in InTemplate
mode_template.wf:39 delegates head-like tags to `mode_in_head`). State: stack [html head
template x N], template stack depth N, mode InTemplate. EOF: InTemplate -34-> InTemplate
N-1 times (reset finds `template` on top, top template mode = InTemplate), the N-th 34
finds `head` not last -> InHead; InHead -13-> AfterHead -21-> InBody; InBody EOF: template
stack empty -> False. Reprocess iterations for the EOF token = N + 2 (N firings of edge 34, then edges 13 and 21). Hence no constant bound exists for
EOF.

### 2d. The bug that was fixed

Commit message of 417f91f: before the fix a foreign-content breakout (`</br>`, `</p>`, or a
breakout start tag) returned True to `process_token`, and the dispatcher, which sends an end
tag at a MathML text / HTML integration point back to foreign content, ran the breakout
again with no state change. In graph terms that was a self-loop with no decrease. The
current code calls `dispatch_html_content` directly (foreign.wf:445, 460), and
`in_body_end_tag` returns False for `br` and `p`, so edges 1-51 contain no self-loop that
leaves mode, stack and template stack unchanged: every edge in section 1 changes `mode`
(target differs from source except edges 34 and 38, which shrink the stack instead).

## 3. The measure

Token classes (the token is fixed across iterations, so its class is a constant):

| class | tokens |
|---|---|
| Chars | any Characters |
| S_col | StartTag `col` |
| S_grp | StartTag caption, colgroup, tbody, tfoot, thead |
| S_tr | StartTag `tr` |
| S_cell | StartTag td, th |
| S_table | StartTag `table` |
| S_other | every other StartTag name |
| E_html | EndTag `html` |
| E_tbl | EndTag table, tbody, tfoot, thead, tr |
| E_other | every other EndTag name |
| Comment, Doctype | edge 39 only |
| EOF | EOF |

Measure, compared lexicographically, all components natural numbers:

    mu = ( N_tmpl , R[class(token)][mode] , D )

- `N_tmpl` = number of html-namespace `template` elements on the stack of open elements
  (the template-mode stack depth `T` works too under the invariant N_tmpl <= T, see
  SUSPECT 2; N_tmpl needs no invariant).
- `R` = the fixed table below, one number per (class, mode).
- `D` = open-element stack depth. Only used to compare equal R.

Rank table R[class][mode] (longest remaining reprocess chain for that class, computed from
the edge model; columns are classes):

| mode | Chars | S_col | S_grp | S_tr | S_cell | S_table | S_other | E_html | E_tbl | E_other | Comment | Doctype | EOF |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Initial | 1 | 5 | 5 | 5 | 5 | 5 | 5 | 6 | 1 | 5 | 0 | 0 | 5 |
| BeforeHtml | 0 | 4 | 4 | 4 | 4 | 4 | 4 | 5 | 0 | 4 | 0 | 0 | 4 |
| BeforeHead | 0 | 3 | 3 | 3 | 3 | 3 | 3 | 4 | 0 | 3 | 0 | 0 | 3 |
| InHead | 0 | 2 | 2 | 2 | 2 | 2 | 2 | 3 | 0 | 2 | 0 | 0 | 2 |
| InHeadNoscript | 1 | 3 | 3 | 3 | 3 | 3 | 3 | 0 | 0 | 3 | 0 | 0 | 3 |
| AfterHead | 0 | 1 | 1 | 1 | 1 | 1 | 1 | 2 | 0 | 1 | 0 | 0 | 1 |
| InBody | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |
| Text | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 6 |
| InTable | 1 | 1 | 0 | 1 | 2 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| InTableText | 0 | 4 | 3 | 2 | 3 | 5 | 1 | 1 | 3 | 1 | 1 | 1 | 1 |
| InCaption | 0 | 2 | 1 | 2 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 |
| InColumnGroup | 0 | 0 | 1 | 2 | 3 | 4 | 1 | 1 | 1 | 1 | 0 | 0 | 0 |
| InTableBody | 1 | 2 | 1 | 0 | 1 | 4 | 0 | 0 | 1 | 0 | 0 | 0 | 0 |
| InRow | 1 | 3 | 2 | 1 | 0 | 4 | 0 | 0 | 2 | 0 | 0 | 0 | 0 |
| InCell | 0 | 4 | 3 | 2 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 |
| InTemplate | 0 | 1 | 1 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 |
| AfterBody | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 0 | 0 |
| InFrameset | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| AfterFrameset | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| AfterAfterBody | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 2 | 1 | 1 | 0 | 0 | 0 |
| AfterAfterFrameset | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

(Text under EOF is 6 because edge 18 can target any mode; the real targets are at most
InHeadNoscript, R = 3, so a tighter 4 would also work. Column S_table has the four SCC
modes InTable/InTableBody/InRow/InColumnGroup at the same rank 4 on purpose.)

Per-edge check (all edges except 34; 207 (edge, class, from, to) tuples checked by
`check.py`, none failing):

- N_tmpl component: non-increasing on every edge (no edge pushes a template element).
  Strictly decreasing on edge 34; there R and D are irrelevant.
- R component, edges 1-33, 35-37, 39-49, 51 and edge 50 for S_grp/S_tr/S_cell/S_other:
  `R[c][from] - R[c][to] >= 1` for every class `c` the edge applies to and every possible
  target. Examples: edge 1 Chars 1 -> 0; edge 5 (S_col) 4 -> 3; edge 28 E_html 1 -> 0; edge 24
  E_other 1 -> 0 and E_tbl 1 -> 0; edge 36 S_col InTable 1 -> InColumnGroup 0; edge 37
  S_tr InTable 1 -> InTableBody 0; edge 40 S_cell InTableBody 1 -> InRow 0; edge 39 S_col
  InTableText 4 -> 1/2/3 (InTable/InTableBody/InRow); edge 29 S_grp InTemplate 1 -> InTable 0;
  edge 18 Text 6 -> at most 5.
- Edges 38 (all three source modes) and 50 for class S_table: for a target outside the SCC
  (InCell 0, InCaption 0, InBody 0, InFrameset 0, InTemplate 1, AfterHead 1, InHead 2,
  BeforeHead 3; all < 4) R drops strictly. For a target inside the SCC R is equal (4 = 4)
  and D drops by >= 1 (the edge pops and does not push), so `mu` still decreases
  lexicographically. These are the only 13 tuples that use D.
- Pushes on edges 5-10, 19-21, 36, 37, 40 and 39 raise D but always lower R, which is
  compared first.

Why no per-mode number independent of the token exists (the "smallest extra fact" is the
token class). The union graph contains the 13 cycles of 2a; each needs opposite orders of
the same two modes:

- InBody vs AfterBody: R(InBody) > R(AfterBody) for `</html>` (edge 28), the reverse for
  every other token (22-24). Token kind alone (Chars/Start/End/EOF) is not enough because
  both are EndTags: the End name `html` is needed.
- InTable vs InColumnGroup (36 vs 50/51), InTable vs InTableBody (37 vs 41/42),
  InTableBody vs InRow (40 vs 43-45): both directions are StartTag; the start-tag name
  class (col / grouping / tr / cell) decides the direction.
- InTable/InTableBody/InRow vs InTableText (35 vs 39): Characters vs non-Characters.

So the smallest fixed tuple over (mode rank, D, T) needs the token class; class boundaries
that matter are exactly S_col, S_grp, S_tr, S_cell, S_table, E_html, E_tbl, plus
Chars/Comment-Doctype/EOF. The reset families need the two extra components: D for
`<table>` (E33 is a cycle in the class graph on which no mode rank can decrease) and
N_tmpl for EOF (E34 lands on any mode).

## 4. Bound per token

Excluding edge 34 and the SCC part of edge 38, the longest chain is the maximum of the R
table: 6 (class E_html from Initial). That is 6 reprocess iterations, 7 dispatches, and it
is attained: the document `</html>` traces Initial -3-> BeforeHtml -6-> BeforeHead -9->
InHead -12-> AfterHead -20-> InBody -28-> AfterBody, where AfterBody consumes `</html>`
(mode_after.wf:63-72). EOF as the first token: 5 (Initial -4-> ... -21-> InBody, False).
Text EOF from `<noscript><style>` (scripting off): 4.

With the two reset families:

- EOF: at most `6 + 4 * N_tmpl0` reprocess iterations (each firing of 34 is followed by at
  most 3 pre-body edges before InBody can fire it again), and exactly `N_tmpl0 + 2` is
  attained (section 2c). Not bounded by a constant; linear in the number of open templates.
- StartTag table: at most `D0 + 5` (one edge 39 from InTableText, at most D0 pops
  around the SCC, at most 4 edges after leaving it via BeforeHead -> InHead -> AfterHead ->
  InBody). I found no input needing more than 1 use of edge 38; a constant bound may hold
  for reachable states but is not proved.
- Nested `process_token` calls (three sites) add at most 3 nested frames per token, each a
  Characters remainder with at most 1 reprocess.

Conclusion: terminates for every (token, state); iterations per token are bounded by 6
except EOF with open templates (linear in N_tmpl) and `<table>` (linear in D).

## SUSPECT

1. foreign.wf:467 `EndOfFile() => ok_val = True()` in `process_foreign_content`. It is
   unreachable only because `should_dispatch_as_html` returns True for EOF (dispatch.wf:93-97);
   if that guard ever changed, `process_token` would spin forever on EOF (no state change,
   returns True). Not a live defect; returning False there removes the dependence.
2. Edge 34 has `pop_template_mode` (types.wf) as a no-op when the template-mode stack is empty
   (mode_template.wf:112 does not check depth). Termination therefore should be argued with
   the stack-side count N_tmpl (guard mode_template.wf:107 + `pop_until_html_tag` guarantee it
   drops), not with `template_modes` depth alone; the T-based version needs the invariant
   N_tmpl <= T, which holds by construction (both are pushed only by mode_head2.wf:120-131
   and popped together at mode_head2.wf:149-158 and edge 34, apart from the fragment
   context template, where T = 1 and N_tmpl = 0) but is not enforced by a check.
3. `S table` in {InTable, InTableBody, InRow, InColumnGroup} is a genuine cycle class in the
   graph (edge 38 and edge 50); only the open-stack depth decreases. Any change that let
   edge 38 or 50 push (for example an implied element) would remove the only decreasing
   component and needs the measure revisited.
4. Recursive `process_token` calls (mode_simple.wf:133, mode_head.wf:80,
   mode_caption_colgroup.wf:127) are outside the loop and make termination depend on
   the argument in section 1 (nesting depth <= 3, mode strictly advances each time).
5. Modelling caveat: the edge table is my hand transcription of the source and the checks
   compare the measure to that transcription; nothing was executed. Independent closure
   check against the source: `grep "let x = True();"` over the 14 mode files
   (mode_initial 4, mode_simple 3, mode_head 3, mode_head2 3, mode_head3 3, mode_head4 5,
   mode_after 6, mode_body_end 1, mode_template 6, mode_table 5, mode_table_body 8,
   mode_caption_colgroup 2; mode_body_main and mode_body_start 0) gives 49 literals, excluding
   the flag assignments (`frameset_ok`, `foster_parenting`, `pending_table_all_whitespace`,
   `pending_lf_skip`) and the unreachable foreign.wf:467. 49 - 2 helper literals + 4 helper
   call sites = 51 = the rows of section 1. Forwarded (non-literal) True results were
   read separately: edges 34, 35, 38 through `mode_in_table`/`mode_in_body`, and edge 28.

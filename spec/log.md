# Specification change log

Newest first. One entry per owner-approved change of the active
specification, `kernel-spec.md`: a `## <date> <title>` heading, `Rules:`
naming every rule added, changed or retired, `Owner-approved:` identifying the
owner's approval in the owner's words, and a concise `Summary:` of the change
and its selection ground. The form is the design tree log's, with `Rules:` in
place of `Nodes:`. The entry is written only after the owner approves, and
`make design-ready` requires a new approved entry whenever the active
specification changes; it cannot tell whether `Rules:` names every changed
rule. Earlier versions are the released archives beside this
file; git holds the rest of the history.

## 2026-09-29 v0.81: element-subtree loop accesses and Segments

Rules: PAR-2, TYPE-2, TYPE-8, TYPE-9, STOR-1, STOR-6, STOR-8, OP-4, OP-9, OP-13, REF-4, MSR-1, PRE-1, and TYPE-9's release-graph paragraph

Owner-approved: 2026-09-29, the owner approved PR #186's handoff, which showed every rule change with its before and after behavior and decision cards Q1 to Q4, writing in Chinese "all agreed", after selecting element-subtree option C and the segmented-storage design earlier in the session.

Summary: PAR-2's element family admits every access at or below one proved affine subscript of an Array, a Slots, a range's run or a Segments. TYPE-9 adds Segments<T>, a Box-only run of segments; OP-4 admits its subscript only as REF-4's &s[i], REF-4 forms &s[i] and &s.all, MSR-1 gives it len, and PRE-1 declares it and box_segments_filled. OP-13 and STOR-8 state that the construction returns None exactly when stride_ceiling(T) * total + 8 * count exceeds 2^62, and STOR-6 qualifies the target for the largest admitted block. The remaining rules add Segments to their shape lists. Grounds: research/investigations/segmented-storage/DESIGN.md.

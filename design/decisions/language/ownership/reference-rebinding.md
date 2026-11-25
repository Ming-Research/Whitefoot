# Reference rebinding

Reference variables that change target: joins, references carried around loops, and passing a reference to a call.

### decision rebound-reference-keeps-all-targets
A reference variable may be rebound, and a join keeps every incoming target, with every check applying to each target.
- rejected: an agent proposed one fixed resolved root for each reference variable; the owner rejected it, because a local name can stand for alternatives without becoming storage or needing a new type.

### decision loop-carried-location-summaries
A loop-carried reference has one finite description per root: the entering static shapes while they agree, and otherwise a containing prefix with an unknown tail.
- replaced: Refusing every path that extends itself around a loop, and so every iterative descent, was on main; the owner changed it, because recursion and indexed pools do not give the iterative cursor edits that owned lists and trees need.
- rejected: an agent proposed promising convergence after one more visit of the body; the owner rejected it, because shortening prefixes over finite roots gives checking that is deterministic and ends, without losing the separations of the entering shapes.
- rejected: an agent proposed treating targets with equal descendant covers as equal; the owner rejected it, because two cursors can select different nodes under the same root, so a cover shows possible interference but never equal targets.

### decision reference-passed-as-an-argument
A reference is passed in the ordinary argument form and checked against the callee's effect row with the argument paths substituted.
- replaced: Child reborrow forms with their own permission, region, suspension and endpoint rules was on main; the owner changed it, because the comparison at the call already decides which accesses may coexist.
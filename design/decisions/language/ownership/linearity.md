# Linearity

Which values must be consumed by an explicit operation, and the ways a program consumes them.

### decision linearity-is-a-type-property
A value is linear when its type is declared `nodrop` or contains such a value at any depth, never because of the scope it is in.
- replaced: Linearity read against the capabilities a scope holds was on main; the owner changed it, because there is one heap every scope can reach, so no scope can lack what a release needs, and a scope-relative class would vary an obligation with no change in what reclaiming needs.

### decision compiler-never-releases-linear-values
The compiler never releases a linear value and refuses a linear binding live on an edge that leaves its scope; it must be moved into a consuming operation or destructured whole.
- rejected: an agent proposed a derived release that could discharge the obligation; the owner rejected it, because the obligation is exactly that some explicit operation consumes the value on every exit path.

### decision linear-window-freed-when-empty
A window or array whose element type is linear is itself linear, and its block is finished by an explicit consuming operation whose contract requires its length to be zero.
- replaced: Leaving out the element edge at a derived release when the length was proved zero was on main; the owner changed it, because the emptiness that makes a block reclaimable is a runtime length the program can prove, so reclamation stays inside the ordinary proof obligation.
- rejected: an agent proposed an unchecked release of the backing alone; the owner rejected it, because a wrong precondition could leak live elements through a second release mechanism outside the proved release walk.

### decision prelude-release-function
The prelude supplies `release<T: drop>(value: T) -> result: unit pure`, an ordinary consuming function whose parameter is released at its end.
- rejected: an agent proposed leaving each project to define its own empty consuming function; the owner rejected it, because Firn wrote the same function three times.
- rejected: an agent proposed a block statement that ends a value early; the owner rejected it, because an affine value is released early by moving it into a consuming function.
- replaced: An early explicit release beside the release at scope exit was on main; the owner changed it, because it was defined over release leaves that spend a store's provider, and with one heap that no value names there is no such leaf.

### decision release-actions-follow-what-runs
A constant-zero Array, Slots or Ring adds no element release action, while its element type keeps its obligation to be consumed explicitly.
- rejected: an agent proposed treating an empty executed release as the drop capability; the owner rejected it, because zero elements do not discharge the element type's obligation to be consumed explicitly.
- rejected: an agent proposed refusing a self-tail transfer only for the release of an element that does not exist; the owner rejected it, because an absent element cannot run a release.
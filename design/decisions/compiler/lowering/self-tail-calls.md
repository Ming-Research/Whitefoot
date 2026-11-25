# Self tail calls

How a direct self call in return position becomes a jump back to the start of the function's body, and which such calls are selected.

### decision a-self-tail-call-jumps-to-a-parameterized-entry-block
A selected self tail call is checked as an ordinary call, then lowered to capturing its arguments, its checked releases and a jump to a body-entry block whose parameters take the new values.
- rejected: an agent proposed lowering self recursion through LLVM's tail calls, by its optimization or by `musttail`; the owner rejected it, because a self transfer needs no inter-function ABI, and the compiler's own control flow guarantees it on every target, even without optimization.
- rejected: an agent proposed a second checker for recursive calls; the owner rejected it, because the existing call judgment must still check the call's requirements and effects.
- rejected: an agent proposed overwriting the parameters one by one while the remaining arguments are evaluated; the owner rejected it, because swaps and aggregate moves must read the previous activation's values before any of them is replaced.

### decision an-unmarked-eligible-self-call-is-also-a-tail-transfer
An unmarked direct self call in the sole return position that meets a `musttail` call's conditions is lowered as one; if one fails, it stays an ordinary call.
- rejected: an agent proposed giving the transfer only to calls marked `musttail`, or refusing an unmarked call that fails the conditions; the owner rejected it, because the conditions make the replacement safe whether or not the writer asked for a guarantee.
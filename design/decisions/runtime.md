# Runtime

The C runtime every compiled program links: waiting contexts, completion, the compute scheduler, the exhaustion floor.

### decision the-runtime-is-hand-reviewed-c
The runtime every compiled program links, with the compute scheduler, the completion runtime and the exhaustion floor, is plain C reviewed by hand, outside the crate's safe-Rust guarantee.
- rejected: an agent proposed writing the runtime in a language that checks memory safety; the owner rejected it, because the runtime speaks the host's threads, rings and ports directly, and it will be replaced with the compiler by self-hosted ones, so that work buys nothing now.
- rejected: an agent proposed a stated policy that confines unsafe code in the runtime; the owner rejected it, because the runtime will be replaced with the compiler by self-hosted ones, so that work buys nothing now, while ordinary engineering quality stays required.

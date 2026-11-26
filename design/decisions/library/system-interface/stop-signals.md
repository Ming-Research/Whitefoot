# Stop signals

How a program hears the host's interrupt and termination requests.

### decision stop-requests-come-through-a-listener
The invocation gives a `StopSignals` capability, and one `StopListener`, costing a handle credit, returns interrupt and termination requests from its waiting `stop_next`.
- rejected: an agent proposed mapping requests onto shared state; the owner rejected it, because every reacting context must then poll, and the runtime must write program state.
- rejected: an agent proposed requests as an input stream that ends; the owner rejected it, because a stream's end hides the request behind an unrelated resource and does not tell interrupt from termination.
- rejected: an agent proposed a handler function the runtime calls; the owner rejected it, because it starts a context the program did not start and runs program code at any point, outside the waiting model.
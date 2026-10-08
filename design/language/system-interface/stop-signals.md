Decision: An invocation receives a StopSignals capability and may spend one handle credit on one linear StopListener whose waiting stop_next returns interrupt or termination requests in runtime observation order, retaining requests observed between waits and treating requests merged by the host before observation as one, with explicit close restoring the host default, because a server needs to run its own orderly stop in a context it starts and host access must be an ordinary parameter, instead of polling program state, ending an unrelated stream or calling program code from a host handler ([investigation](../../../research/investigations/stop-signals/README.md#proposal)).

Rejected:
- B, requests mapped onto shared state: rejected because every reacting context must poll and the runtime must write program state.
- C, requests as an ending input stream: rejected because stream end hides the request behind an unrelated resource and does not distinguish interrupt from termination.
- D, a handler function called by the runtime: rejected because it starts a context the program did not start and runs program code at an arbitrary point outside the waiting model.

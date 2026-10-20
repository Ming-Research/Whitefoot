Decision: Snapshots have an explicit reserve and abort on exhaustion, preserving the previous authoritative base and log with bounded cleanup and consumer-set pause/latency targets, because arbitrary replacement can exhaust a small retention reserve and service availability takes priority over persistence progress ([overwrite witness](../../../../research/investigations/consistent-snapshots/README.md#need-evidence-and-a-limit-no-mechanism-removes)), instead of guaranteeing completion by delaying writers or making spill the default.

Rejected:
- Guaranteeing completion by throttling or stopping writers: rejected because it trades service availability for persistence progress.
- Spilling retained versions or logs to storage or a replica by default: rejected because it adds I/O, space and failure modes; it may be added later as an explicit deployment policy over the reserve-and-abort contract.

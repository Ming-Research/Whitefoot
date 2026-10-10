Decision: Frozen datasets begin as an opt-in persistent library using SHARE-1 immutable shared nodes, retained roots and replacement paths; consider a new language storage domain only if a prototype finds an operation existing ownership rules cannot express, because the [ownership witness](../../../research/investigations/consistent-snapshots/README.md#routes-and-their-costs) supports that representation but complete dataset expressibility remains unprototyped, instead of adding storage, ownership and proof rules up front.

Decision: Fork stays outside the source abstraction, with restricted fork only a possible backend behind the dataset abstraction, because the [child execution contract](../../../research/investigations/consistent-snapshots/README.md#fork-is-an-execution-model-question) and Windows/MMU-less coverage remain unresolved, instead of exposing fork continuations for dataset capture.

Rejected:
- A new language storage domain up front: rejected because no operation has yet been shown inexpressible under the existing rules.
- Restricted fork as the source abstraction: rejected because it needs a complete child execution contract and another route on Windows and MMU-less targets.
- General fork returning into both continuations: rejected because it adds context, handle and platform contracts without a consumer.

# Specification studies

Three studies of Whitefoot, translated into English from the Chinese originals. Agents that redesign the specification read
them.

## How they were made

Three agents wrote them on 2026-11-12, one study each. Each agent read read-only copies of the repositories at their `main`
(Whitefoot at `55b6fd7a7`). No agent compiled anything or ran any program or test. Where a claim needs a machine to confirm
it, the study marks it. The simpler-language and checker-soundness studies mark such claims "unverified". The spec-proof study
marks each conclusion "[checked]", "[inferred]" or "[not looked up]". The translations keep those marks.

Each file here is the study's result, translated whole. The studies' working notes (`process.md`) are not kept here. Where a
study refers to its `process.md`, that reference points to notes this record does not hold.

## The three studies

- [simpler-language.md](simpler-language.md): which parts of the language's design force the rules to carry special cases,
  and how a different definition of those parts would make the special cases unnecessary.
- [spec-proof.md](spec-proof.md): what stands in the way of proving the whole specification sound in Lean, and the cheapest
  way through.
- [checker-soundness.md](checker-soundness.md): whether the checker does what the specification says, and which kind of
  evidence (runtime comparison, a small certificate verifier, adversarial review) to take for it.

## What reads them

On 2026-11-25 the owner opened a redesign of the whole specification. The agents doing that redesign read these three
studies.

# Complete constant initialization

## Question and scope

Reassess the complete-initialization portion of
[CONST-2](../../../spec/kernel-spec.md) after retirement of its constitutional
T1 citation. Immutable lookup tables are an existing compiler workload, including
[the base64 encoder](../../../tests/conformance/cases/x-base64-rfc-vectors-run.wf).
The question is whether every component must have a defined value, and whether
safety alone requires every value to be written explicitly.

This assessment covers initialization coverage and omission handling only.
It does not select const eligibility, lifetime, borrow rules, all construction
syntax, or general partial-storage mechanisms. The current specification remains
the acceptance authority while alternatives are considered.

## Required properties and alternatives

The constitution requires exclusion of uninitialized reads and machine-checked
safety before acceptance. A complete immutable value exposes readable components;
each must therefore be initialized before such access. This is a conditional
deduction about accessible values, not proof that one source notation is necessary.

| Alternative | Safety and authoring consequence | Selection ground |
|---|---|---|
| Require complete typed initializers, rejecting omissions | Every published component has an explicit value; a missing field or array entry cannot silently acquire one. Exact-type constant references can supply a complete value. | Retain the existing form provisionally: omission rejection directly serves the harness objective. This is not measured superiority over all other writing forms. |
| Supply specified default values for omitted components | Can meet initialization safety if every default is defined and valid for its type. An omitted component may then be an accepted logic error instead of a rejected omission. | Not selected for this form. A concrete need or comparative authoring evidence may justify explicit defaulting syntax; defaults are not inherently unsafe. |
| Publish incomplete storage with restricted access until initialized | Can be safe with separate state and access obligations, but does not yet provide the same complete immutable value. | Outside this constant-value decision; it belongs to storage-state design and is not prohibited by this reasoning. |

Named fields and declaration order do not establish that the assigned values
are intended. Swapping two same-typed values under correct labels remains a
possible logic error. The historical construction rationale cannot establish
that all transpositions become compile-time rejections.

## Discriminating checks

Criteria recorded before these checks on 2026-09-09:

- The existing complete struct-constant fixture must compile and return success.
- Removing one field must produce the owning constant/construction rejection,
  rather than successful default initialization or an implementation failure.
- Swapping the two u64 field values while retaining their labels must compile;
  the fixture's independent value checks should then return failure. If it is
  rejected solely for the swap, investigate what additional contract explains it.
- Existing array checks must accept exactly filled constants and reject a short
  initializer. Acceptance of a short initializer would require investigation of
  a specification/compiler discrepancy, not changing the expected verdict.

Use the ordinary compiler on
[const2-pos-struct-const.wf](../../../tests/conformance/cases/const2-pos-struct-const.wf),
with the missing-field and swapped-value variants derived from its initializer.
Run the existing `semantic::tests::arrays::constants_fill_length_and_index_share_exact_run_types`
and `semantic::tests::arrays::const_expression_and_const_value_failures_keep_their_rule_owners`
tests. No new language rule or special compiler path is needed for these probes.

These checks distinguish rejection, safe initialization, and intended values;
they do not compare agent productivity, generated-code performance, or all
possible defaulting designs. Reopen the explicit-coverage choice when a concrete
constant workload or comparative writer trial shows a safer or more effective
form under the same intended-value and initialization requirements.

## Results and disposition

Run on 2026-09-09, Darwin arm64 with Apple Clang 21.0.0. The criteria are
preserved in commit `8da2c5cc`, before execution. The compiler source and active
specification were unchanged by this assessment. Build with
`cargo build --profile gate --bin whitefootc --locked --offline` in `compiler/`,
then compile each fixture through `whitefootc source.wf -o program` and run it.

| Fixture initializer | Compile result | Execution result |
|---|---|---|
| `Window(width: 3_u64, height: 2_u64)` (unchanged fixture) | Accepted | Exit 0 |
| `Window(width: 3_u64)` | GRAM-8 `InvalidConstructionFields`, declared fields `width`, `height` | No executable |
| `Window(width: 2_u64, height: 3_u64)` (initializer only; checks unchanged) | Accepted | Exit 1 from the fixture's independent expected-value check |

`cargo test --profile gate --lib semantic::tests::arrays:: --locked --offline`
passed all seven existing tests, including the two named above. This verifies
the selected array witnesses, not every constant shape.

The implementation follows the distinction: array initializer length is checked
before element construction, and struct field coverage/order is checked before
building the complete value in
[constant checking](../../../compiler/src/semantic/check/types.rs). The result
supports retaining complete explicit coverage as the current omission-rejection
choice. It does not demonstrate that a defaulting form would be unsafe, slower,
or harder for an agent to use.

The affected set is CONST-2's coverage requirement, its GRAM-8 construction
dependency, and the [construction rationale](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/surface-form/construction-form.md).
The [historical grounds assessment](../decision-workflow/RULE-GROUNDS.md#types-ownership-and-storage)
also examined CONST-2's eligibility, lifetime, representation, and read rules;
those arguments are not results of this initialization experiment or a current
rule inventory.
GRAM-8's general naming/order selection remains provisional without comparative
authoring evidence. The retired rule index linked that assessment and this
study's bounded initialization result; it is no longer a current authority.
The active specification, compiler, and conformance expectations need no change.

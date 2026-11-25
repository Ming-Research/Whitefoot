# The Whitefoot constitution

This constitution holds what Whitefoot is for and its lasting values, and every
decision in the project comes from them. The owner sets it and changes it.

## What Whitefoot is for

Whitefoot is a systems programming language designed as a harness for AI agents.
People delegate implementation to agents and keep control of the software's
objectives and key tradeoffs: requirements that can be formalized are enforced
by machine-checked constraints, so that people need not read generated code
again and again. A program Whitefoot accepts is proved safe, and the same proofs
make it fast and let it use the whole machine.

The language supports building, verifying, maintaining and evolving software at
the scale of its target projects. It aims at large systems, such as kernels,
compilers and browsers, and at small embedded systems too. It is proven first on
real programs that redo existing systems better.

Its purpose does not settle its objectives or its mechanisms by itself: the
articles below are choices, and each concrete design needs grounds of its own.

## Article 1 (safe). Safety is proved before a program is accepted

An accepted program has no undefined behavior, memory corruption, data race,
uninitialized read, silent overflow, or any other operation whose safety
conditions no machine proof has established. The required guarantees are
established by machine verification before a program is accepted, and they are
constraints that performance and every other objective respect.

The specification defines the execution model and the external conditions the
guarantees rest on. The compiler and the runtime implement them, and a defect of
theirs does not change what the language requires. The guarantees do not show
that the requirements are right or complete: logic errors, unintended
nontermination included, may remain. Expected input and environment failures
have defined behavior. Where a use has explicit resource budgets and the
conditions apply, the language supports machine-verifiable bounds on memory and
other hardware resources.

Violations: an operation accepted without a proof of its safety conditions; a
speed-up bought with an unchecked operation; any way to turn the checks off.

## Article 2 (fast). Runtime performance comes before ease of writing

The language gives what its target projects need to meet their performance
requirements; their goals and tradeoffs are set by each project and use. Within
the safety the language requires and the practical feasibility of development,
runtime performance comes before ease of writing and speed of compiling. Ease of
writing by hand and familiar syntax may be given up for the language's
objectives. More verbose source, more proof work and more compiling work are
acceptable, and the compiler is improved to reduce them. Compiling and proof
checking stay fast enough for practical iteration at the target projects' scale;
that they terminate is not enough.

Constraints and guidance for writers serve these objectives: a restriction that
excludes a faster implementation meeting the safety and feasibility requirements
is revised or replaced. Concrete mechanisms are chosen by the target projects'
requirements, technical arguments and experimental evidence.

Violations: guidance that excludes a faster safe implementation, kept as it is;
compile or proof times that stop practical iteration at a target project's
scale.

## Article 3 (parallel). A program uses the whole machine

A program uses the machine's parallelism, for computation and for I/O alike, and
keeps every safety guarantee in doing so. Parallelism the compiler derives from
its proofs gives the result the sequential program gives; concurrency a program
states is proved free of data races like any other operation.

Violations: parallel execution whose result can differ from the sequential
program's without the program stating it; independent work that cannot run in
parallel because the language cannot state or prove its independence, left
unreported.

## Article 4 (ordinary objects). I/O is ordinary program state

I/O and external resources are represented through the same object, ownership,
borrowing, effect and proof abstractions as the rest of a program's state, and
their required relations belong in ordinary contracts. An external role grants
no exception in the source language: no declaration, type, ownership, effect,
proof, diagnostic or semantic rule tells a value or a function apart by whether
its implementation crosses the host boundary.

Violations: a rule, a type or a diagnostic that treats a value or a function
differently because its implementation crosses the host boundary.

## Article 5 (real programs). Whitefoot is proven by real programs

Whitefoot is proven by real programs that redo existing systems better: safe
from the source, using the whole machine, and faster than the best existing
implementations. Such a program exists first for Whitefoot. What it cannot write
well or make fast is a gap of Whitefoot, fixed in the language, the compiler or
the guidance agents read; a fault of the program itself is fixed in the program.
Agents learn Whitefoot from the compiler's rejections and its guide, so a
rejection leads to the accepted form.

Violations: a gap a real program met that Whitefoot never heard of; a gap hidden
by a workaround in the guide or by leaving the workload out; a rejection that
does not lead to an accepted form, left as it is.

## Article 6 (evolve). Everything here may change as the grounds change

Backward compatibility may yield to the language's objectives and its long-term
evolution. Once real projects have compatibility needs, an improvement is
weighed against its actual impact and the means of migrating. Objectives,
tradeoffs and design choices, this constitution's included, are reassessed as AI
capabilities, evidence and expectations about how software will be made change
their grounds.

Violations: an improvement refused only to keep old source working, before any
real project needs it.

## Article 7 (simple). The language and its specification stay small and regular

The language and its specification stay as small as their objectives allow. A
capability enters through a design that fits the language as a whole, not as a
patch for one need: a rule states its fact once and holds without exceptions,
and a new form or type extends what the rules already say instead of adding a
case to each of them. Where this article conflicts with Article 1 or Article 2,
safety comes first and performance second.

Violations: a rule changed for one need with an exception or a listed case; a
capability added without a design that fits the language as a whole; a
specification that grows by special cases.

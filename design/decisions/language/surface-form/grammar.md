# Grammar

The grammar of Whitefoot source and how its syntax errors are reported.

### decision a-strong-ll2-grammar
The grammar is deterministic, unambiguous and context-free, read by a generated strong LL(2) parser whose lexer decides a terminal by token shape and exact spelling alone.
- rejected: an agent proposed a richer parser: unbounded-lookahead recursive descent, PEG, GLR, LALR tables, Pratt parsing or keyword-priority lexing; the owner rejected it, because the language is meant to be simple and fast to compile, and a grammar richer for a writer's comfort buys nothing.

### decision syntax-errors-per-declaration
Syntax checking runs on its own for each segment that starts at a column-zero top-level declaration keyword or a graph-record row, and reports one defect per segment in source order.
- rejected: an agent proposed recovery over the whole text that inserts or skips tokens anywhere, as rustc does; the owner rejected it, because such recovery produces cascades of false errors and needs a separate set of recovery rules.
- replaced: One syntax error per compile was on main; the owner changed it, because a writer who fixes the one reported defect each round needs n rounds for n independent declaration defects, while one rejection per declaration shows them together.

### decision expected-tokens-come-from-the-parse-stack
The expected terminals of a syntax error come from the rest of the productions actually on the parse stack, while recognition keeps the generated strong LL(2) grammar.
- replaced: Taking expected tokens from grammar-wide FOLLOW sets was on main; the owner changed it, because they list tokens impossible at the position, such as `=`, `;` and `)` after a function's single result type, and so point the writer to forms the grammar cannot derive there.

### decision match-arms-share-one-production
A match arm is one production with a flat choice between a variant label, a literal, an IDENT and `else`, and the checker picks the class the scrutinee's type needs.
- rejected: an agent proposed separate match productions competing in the grammar, or a parser that resolves a constant name to recognize an arm; the owner rejected it, because the four arm starts have disjoint terminal predicates and need no new lexer class or semantic lookahead.

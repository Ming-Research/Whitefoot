#![allow(clippy::panic)]

use super::{
    DecisionKind, GrammarNodeKind, LookaheadPredicate, Production, diagnostic_terminal_order,
    grammar_node, productions,
};
use crate::syntax::terminal::{
    ALL_FIXED_TERMINALS, ALL_TERMINAL_PREDICATES, FixedTerminal, TerminalPredicate,
};

use super::generated::{DECISIONS, SELECT_ROWS};

/// An independent linear scan of the provenance rows checks every generated
/// lookup bucket, including absent predicates and the source-end sentinel.
#[test]
fn first_predicate_index_preserves_every_original_row_in_order() {
    for decision in DECISIONS {
        for predicate in ALL_TERMINAL_PREDICATES
            .into_iter()
            .map(LookaheadPredicate::Terminal)
            .chain([LookaheadPredicate::SourceEnd])
        {
            let expected: Vec<_> = decision
                .rows()
                .iter()
                .filter(|row| row.position(0).unwrap().predicate() == predicate)
                .collect();
            let actual: Vec<_> = decision.rows_starting_with(predicate).collect();
            assert_eq!(actual, expected, "{decision:?}, {predicate:?}");
        }
    }
}

/// Compare the indexed selector with the original complete-row judgment on
/// every two-token combination, every one-token tail, and source end. Real
/// classification retains `unit`'s simultaneous fixed/literal membership.
#[test]
fn indexed_selection_matches_a_linear_scan_for_all_token_pairs() {
    use crate::syntax::parser::{DecisionSelection, Work, select_arm};
    use crate::{
        ACTIVE_KERNEL_SPEC_HASH, ClassifiedToken, CompilerLimits, LexOutcome, SourceBundle,
        SourceInput, TerminalOutcome, classify_terminals, lex,
    };

    let mut source = ALL_FIXED_TERMINALS
        .into_iter()
        .map(FixedTerminal::spelling)
        .collect::<Vec<_>>()
        .join(" ");
    source.push_str(" name Type @label cvt.wrap 0_u64 \"text\" 17");
    let limits = CompilerLimits::default();
    let bundle = SourceBundle::with_limits(
        &[SourceInput::new("lookahead.wf", source.as_bytes())],
        limits.source,
    )
    .unwrap();
    let LexOutcome::Complete(lexed) = lex(&bundle, limits.lexer) else {
        panic!("terminal witnesses must lex");
    };
    let TerminalOutcome::Complete(classified) =
        classify_terminals(&lexed, ACTIVE_KERNEL_SPEC_HASH, limits.terminals)
    else {
        panic!("terminal witnesses must classify");
    };
    for predicate in ALL_TERMINAL_PREDICATES {
        assert!(
            classified
                .tokens()
                .iter()
                .any(|token| token.terminals().contains(predicate))
        );
    }

    let check = |tokens: &[ClassifiedToken]| {
        let accepts = |predicate: LookaheadPredicate, position: usize| match (
            tokens.get(position),
            predicate,
        ) {
            (Some(token), LookaheadPredicate::Terminal(terminal)) => {
                token.terminals().contains(terminal)
            }
            (None, LookaheadPredicate::SourceEnd) => true,
            _ => false,
        };
        for decision in DECISIONS {
            let mut expected = None;
            for row in decision.rows() {
                if accepts(row.position(0).unwrap().predicate(), 0)
                    && accepts(row.position(1).unwrap().predicate(), 1)
                {
                    match expected {
                        Some(Ok(arm)) if arm != row.arm() => {
                            expected = Some(Err(()));
                            break;
                        }
                        Some(_) => {}
                        None => expected = Some(Ok(row.arm())),
                    }
                }
            }
            let actual = match select_arm(decision, tokens, 0, &mut Work::new(u64::MAX)) {
                Ok(DecisionSelection::NoMatch) => None,
                Ok(DecisionSelection::Arm(arm)) => Some(Ok(arm)),
                Ok(DecisionSelection::Conflict) => Some(Err(())),
                Err(_) => panic!("generated decision must select with ample limits"),
            };
            assert_eq!(actual, expected, "{decision:?}, {tokens:?}");
        }
    };
    check(&[]);
    for first in classified.tokens() {
        check(&[*first]);
        for second in classified.tokens() {
            check(&[*first, *second]);
        }
    }
}

/// Pin the complete grammar inventory, including the optional call-site
/// `musttail` marker [GRAM-5, FN-10]. Its presence adds one decision and one
/// terminal; its expression starts also expand the two-position select rows.
#[test]
fn complete_inventory_is_pinned() {
    assert_eq!(productions().len(), 95);
    // v0.72's standard library qualifier [MOD-10] adds a `"pkg" | "std"`
    // alternation to `alias_decl`, `module_path` and `callee`, three
    // decisions, and extends `type_path`'s existing root alternation.
    // v0.76 moves reference access from an expression start into psuffix;
    // v0.77 adds the optional `waits` of `fn_decl` and `fn_sig` [WAIT-1],
    // the optional `mustpar` of `for_stmt`, and the alternation of the
    // call marker `("musttail" | "mustpar")?` [PAR-4]: four decisions.
    // v0.82 retires `mustpar` with PAR-4: `for_stmt` loses its optional
    // marker, one decision, and `spawn` [WAIT-3] takes its place in the
    // call marker's alternation.
    // v0.79 adds a STRING arm to the existing `cvalue` alternation
    // [CONST-2]: no decision, and nine rows for its continuations.
    // v0.82's `atomic_stmt` adds its optional guard and its statement
    // repeat [GRAM-4, SHARE-2]: two decisions.
    // v0.82's `type_invariant*` repeat closes `struct_decl` [GRAM-2, TYPE-11]:
    // one decision, which moves every later decision by one.
    // v0.84's `loop_rank` [TERM-1] adds one production and the choice
    // between it and a `header_invariant` as the loop header's first item,
    // and its `("may_wait" | "must_wait")?` [WAIT-1] adds to `fn_decl` and
    // `fn_sig` the choice between the two waiting kinds: two decisions.
    assert_eq!(DECISIONS.len(), 166);
    assert_eq!(SELECT_ROWS.len(), 7_340);
    assert_eq!(diagnostic_terminal_order().len(), 113);
    assert_eq!(productions()[0], Production::Program);
    // v0.70 [GRAM-2] adds the file alias header as an `item` arm and closes
    // with the module graph productions; [GRAM-3] adds `type_path` and
    // [GRAM-5] `callee_path`, so definition positions after each move.
    assert_eq!(productions()[1], Production::Item);
    assert_eq!(productions()[2], Production::AliasDecl);
    assert_eq!(productions()[3], Production::HeapDecl);
    assert_eq!(productions()[13], Production::ContractDefine);
    assert_eq!(productions()[14], Production::RequiresClause);
    assert_eq!(productions()[15], Production::EnsuresClause);
    assert_eq!(productions()[16], Production::ResultRoute);
    assert_eq!(productions()[27], Production::CapabilityBound);
    assert_eq!(productions()[30], Production::GraphFile);
    assert_eq!(productions()[33], Production::EntryDecl);
    assert_eq!(productions()[35], Production::TypePath);
    assert_eq!(productions()[49], Production::LoopRank);
    assert_eq!(productions()[50], Production::ForStmt);
    assert_eq!(productions()[51], Production::ForBinding);
    assert_eq!(productions()[52], Production::HeaderInvariant);
    assert_eq!(productions()[53], Production::InvariantStmt);
    assert_eq!(productions()[54], Production::TypeInvariant);
    assert_eq!(productions()[55], Production::ProofUse);
    assert_eq!(productions()[56], Production::UsePremise);
    assert_eq!(productions()[72], Production::CompareOp);
    assert_eq!(productions()[76], Production::CalleePath);
    assert_eq!(productions()[81], Production::ClauseExpr);
    assert_eq!(productions()[82], Production::ClauseOp);
    assert_eq!(productions()[86], Production::RangeTail);
    assert_eq!(productions()[90], Production::Effect);
    assert_eq!(productions()[91], Production::EffectPath);
    assert_eq!(productions()[92], Production::Epbase);
    assert_eq!(productions()[93], Production::Epsuffix);
    assert_eq!(productions()[94], Production::Erange);
    assert_eq!(Production::ForStmt.index(), 62);
    assert_eq!(Production::ForBinding.index(), 63);
    assert_eq!(Production::HeaderInvariant.index(), 64);
    assert_eq!(Production::RequiresClause.index(), 65);
    assert_eq!(Production::EnsuresClause.index(), 66);
    assert_eq!(Production::ResultRoute.index(), 67);
    assert_eq!(Production::EffectPath.index(), 68);
    assert_eq!(Production::InvariantStmt.index(), 69);
    assert_eq!(Production::AffineExpr.index(), 70);
    assert_eq!(Production::AffineTerm.index(), 71);
    assert_eq!(Production::AffineFactor.index(), 72);
    assert_eq!(Production::AffineAddOp.index(), 73);
    assert_eq!(Production::ProofUse.index(), 74);
    assert_eq!(Production::ClauseExpr.index(), 76);
    assert_eq!(Production::ClauseOp.index(), 77);
    assert_eq!(Production::CapabilityBound.index(), 78);
    // Retiring `mode` removes its dense slot; surviving productions keep
    // their relative order, including the appended grammar extensions.
    assert_eq!(Production::HeapDecl.index(), 79);
    assert_eq!(Production::RangeTail.index(), 80);
    assert_eq!(Production::Epbase.index(), 81);
    assert_eq!(Production::Epsuffix.index(), 82);
    assert_eq!(Production::Erange.index(), 83);
    assert_eq!(Production::UsePremise.index(), 84);
    // The v0.70 module productions append after every earlier dense slot.
    assert_eq!(Production::AliasDecl.index(), 85);
    assert_eq!(Production::GraphFile.index(), 86);
    assert_eq!(Production::ModuleRow.index(), 87);
    assert_eq!(Production::ModulePath.index(), 88);
    assert_eq!(Production::EntryDecl.index(), 89);
    assert_eq!(Production::TypePath.index(), 90);
    assert_eq!(Production::CalleePath.index(), 91);
    assert_eq!(Production::TypeInvariant.index(), 93);
    // Index 2 is `struct_decl`'s `"opaque"?` optional [GRAM-2, TYPE-2], so
    // every decision after `item` follows that optional before
    // that modifier entered the grammar.
    // `item`'s alias, public-item and declaration choice and the public
    // item's declaration group precede the alias header's own decisions,
    // among them v0.72's `"pkg" | "std"` root [MOD-10], which moves every
    // later decision by one; `module_path`'s and `callee`'s own root
    // alternations move the decisions after them by one more each.
    assert_eq!(DECISIONS[1].production(), Production::Item);
    assert_eq!(DECISIONS[1].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[2].production(), Production::Item);
    assert_eq!(DECISIONS[2].kind(), DecisionKind::Choice);
    // [GRAM-2, TYPE-11]: `struct_decl` closes with its `field*` repeat and then
    // v0.82's `type_invariant*` repeat.
    assert_eq!(DECISIONS[12].production(), Production::StructDecl);
    assert_eq!(DECISIONS[12].kind(), DecisionKind::Repeat0);
    assert_eq!(DECISIONS[13].production(), Production::StructDecl);
    assert_eq!(DECISIONS[13].kind(), DecisionKind::Repeat0);
    // [GRAM-2, TYPE-2, MOD-5]: `field := "public"? "readonly"? IDENT ":" type ";"`
    // owns two optionals of its own.
    assert_eq!(DECISIONS[14].production(), Production::Field);
    assert_eq!(DECISIONS[14].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[15].production(), Production::Field);
    assert_eq!(DECISIONS[15].kind(), DecisionKind::Optional);
    // x1 [EFF-1]: `epbase := IDENT` has one alternative and owns no decision.
    assert!(
        !DECISIONS
            .iter()
            .any(|decision| decision.production() == Production::Epbase)
    );
    // v0.84's waiting kind [WAIT-1] adds to `fn_decl` the choice between
    // `may_wait` and `must_wait`, so its closing `stmt*` now stands where the
    // contract block's first repeat stood; `fn_sig` gains the same choice
    // after the result route, so every later decision moves by two.
    assert_eq!(DECISIONS[33].production(), Production::FnDecl);
    assert_eq!(DECISIONS[33].kind(), DecisionKind::Repeat0);
    assert_eq!(DECISIONS[34].production(), Production::ContractBlock);
    assert_eq!(DECISIONS[34].kind(), DecisionKind::Repeat0);
    assert_eq!(DECISIONS[35].production(), Production::ContractBlock);
    assert_eq!(DECISIONS[35].kind(), DecisionKind::Repeat0);
    assert_eq!(DECISIONS[36].production(), Production::ContractBlock);
    assert_eq!(DECISIONS[36].kind(), DecisionKind::Repeat0);
    assert_eq!(DECISIONS[37].production(), Production::EnsuresClause);
    assert_eq!(DECISIONS[37].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[38].production(), Production::ResultRoute);
    assert_eq!(DECISIONS[38].kind(), DecisionKind::Optional);
    // Value mode follows a written type; `&` selects either one referent or
    // a range. Both choices belong to `param` [GRAM-2, REF-4].
    assert_eq!(DECISIONS[63].production(), Production::Param);
    assert_eq!(DECISIONS[63].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[64].production(), Production::Param);
    assert_eq!(DECISIONS[64].kind(), DecisionKind::Choice);
    // `type` keeps its primitive-or-nominal choice, now with the qualified
    // `type_path` arm [GRAM-3, MOD-3], and one `targs?` optional per nominal
    // arm; v0.59's three shape optionals retired with `array`, `box` and
    // `arena`.
    assert_eq!(DECISIONS[72].production(), Production::Type);
    assert_eq!(DECISIONS[72].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[73].production(), Production::Type);
    assert_eq!(DECISIONS[73].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[74].production(), Production::Type);
    assert_eq!(DECISIONS[74].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[95].production(), Production::LoopStmt);
    assert_eq!(DECISIONS[95].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[96].production(), Production::LoopStmt);
    assert_eq!(DECISIONS[96].kind(), DecisionKind::Optional);
    // v0.84 [TERM-1]: the header's first item chooses between a
    // `loop_rank` and a `header_invariant`, which moves every later decision
    // by one.
    assert_eq!(DECISIONS[97].production(), Production::LoopStmt);
    assert_eq!(DECISIONS[97].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[98].production(), Production::LoopStmt);
    assert_eq!(DECISIONS[98].kind(), DecisionKind::Repeat0);
    assert_eq!(DECISIONS[100].production(), Production::ForStmt);
    assert_eq!(DECISIONS[100].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[101].production(), Production::ForStmt);
    assert_eq!(DECISIONS[101].kind(), DecisionKind::Repeat0);
    assert_eq!(DECISIONS[103].production(), Production::InvariantStmt);
    assert_eq!(DECISIONS[103].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[104].production(), Production::InvariantStmt);
    assert_eq!(DECISIONS[104].kind(), DecisionKind::Repeat1);
    assert_eq!(DECISIONS[107].production(), Production::UsePremise);
    assert_eq!(DECISIONS[107].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[112].production(), Production::BreakStmt);
    assert_eq!(DECISIONS[112].kind(), DecisionKind::Optional);
    // The call-site marker optional, and v0.82's `atomic_stmt` guard and
    // statement repeat, shift the later place/effect choices; v0.82's
    // retired `for_stmt` marker moves every decision after it back by one.
    assert_eq!(DECISIONS[128].production(), Production::Call);
    assert_eq!(DECISIONS[128].kind(), DecisionKind::Optional);
    // `psuffix` carries the field, payload and index-or-range choice, and the
    // factored `range_tail?` that keeps the index and range steps
    // strong-LL(2) [GRAM-1, GRAM-5].
    assert_eq!(DECISIONS[148].production(), Production::Psuffix);
    assert_eq!(DECISIONS[148].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[149].production(), Production::Psuffix);
    assert_eq!(DECISIONS[149].kind(), DecisionKind::Optional);
    // `epsuffix` mirrors it inside an effect row [EFF-1].
    assert_eq!(DECISIONS[164].production(), Production::Epsuffix);
    assert_eq!(DECISIONS[164].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[165].production(), Production::Epsuffix);
    assert_eq!(DECISIONS[165].kind(), DecisionKind::Optional);
}

/// `borrow_expr` is `"&" place` and owns no decision.
///
/// v0.59 wrote `"&" ("uniq")? (REGIONID)? place`, whose two optionals were the
/// last grammar-visible trace of the permission marker and of regions. Their
/// absence is the property, so it is asserted directly rather than left to the
/// inventory count above [GRAM-5, REF-1].
#[test]
fn borrow_expr_owns_no_decision() {
    assert!(
        !DECISIONS
            .iter()
            .any(|decision| decision.production() == Production::BorrowExpr)
    );
}

#[test]
fn active_inventory_has_only_ordinary_function_parameters() {
    assert!(productions().contains(&Production::FnDecl));
    assert!(productions().contains(&Production::FnSig));
    assert!(FixedTerminal::from_spelling(b"command").is_none());
    assert!(FixedTerminal::from_spelling(b"as").is_none());
}

/// Retired atoms are ordinary names and the current keywords are present.
///
/// `from_spelling` is what decides whether a word is a keyword or a name, so
/// a retired atom left in the inventory would keep stealing its spelling from
/// IDENT or TYPEID without any decision table noticing [GRAM-1, FORM-3].
#[test]
fn the_retired_atoms_leave_the_inventory_and_the_new_ones_enter() {
    for retired in [
        b"region".as_slice(),
        b"uniq",
        b"dispose",
        b"replace",
        b"allocates",
        b"array",
        b"box",
        b"arena",
        b"buffer",
        b"Slice",
        b"MutSlice",
        b"formal",
        b"actual",
        b"linear",
        b"affine",
        b"own",
    ] {
        assert!(
            FixedTerminal::from_spelling(retired).is_none(),
            "retired atom still in the inventory: {retired:?}"
        );
    }
    assert_eq!(
        FixedTerminal::from_spelling(b"program"),
        Some(FixedTerminal::Program)
    );
    assert_eq!(
        FixedTerminal::from_spelling(b"no_heap"),
        Some(FixedTerminal::NoHeap)
    );
    assert_eq!(
        FixedTerminal::from_spelling(b"readonly"),
        Some(FixedTerminal::Readonly)
    );
    assert_eq!(
        FixedTerminal::from_spelling(b"musttail"),
        Some(FixedTerminal::Musttail)
    );
    for (spelling, terminal) in [
        (b"public".as_slice(), FixedTerminal::Public),
        (b"alias".as_slice(), FixedTerminal::Alias),
        (b"pkg".as_slice(), FixedTerminal::Pkg),
        (b"std".as_slice(), FixedTerminal::Std),
    ] {
        assert_eq!(FixedTerminal::from_spelling(spelling), Some(terminal));
    }
    assert_eq!(ALL_FIXED_TERMINALS.len(), 106);
    // No fixed atom is capitalized any more, so nothing competes with TYPEID.
    assert!(ALL_FIXED_TERMINALS.iter().all(|terminal| {
        !terminal
            .spelling_bytes()
            .first()
            .is_some_and(u8::is_ascii_uppercase)
    }));
}

#[test]
fn fixed_terminal_inventory_follows_first_grammar_occurrence() {
    let derived = diagnostic_terminal_order()
        .iter()
        .filter_map(|predicate| match predicate {
            LookaheadPredicate::Terminal(TerminalPredicate::Fixed(terminal)) => Some(*terminal),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(derived.as_slice(), ALL_FIXED_TERMINALS.as_slice());
}

#[test]
fn every_decision_has_two_position_rows_and_complete_arm_coverage() {
    let mut decisions = 0_usize;
    for production in productions() {
        let mut stack = vec![production.root()];
        while let Some(node_id) = stack.pop() {
            let Some(node) = grammar_node(node_id) else {
                panic!("generated node must exist");
            };
            if let Some(decision) = node.decision() {
                decisions += 1;
                let mut seen = vec![false; usize::from(decision.arm_count())];
                for row in decision.rows() {
                    assert!(row.position(0).is_some());
                    assert!(row.position(1).is_some());
                    seen[usize::from(row.arm())] = true;
                }
                assert!(seen.into_iter().all(|value| value));
            }
            stack.extend_from_slice(node.children());
        }
    }
    // The same 166 decisions `complete_inventory_is_pinned` reads out of the
    // generated table, counted a second time by walking every production's
    // node tree. `struct_decl`'s `"opaque"?` optional [GRAM-2, TYPE-2] is
    // reachable from `item`, so the walk and the table agree on it; a
    // decision in the table that no production reaches would show up as the
    // two counts disagreeing.
    assert_eq!(decisions, DECISIONS.len());
    assert_eq!(decisions, 166);
}

#[test]
fn program_is_one_repeat_decision_over_items() {
    let Some(root) = grammar_node(Production::Program.root()) else {
        panic!("program root must exist");
    };
    assert_eq!(root.kind(), GrammarNodeKind::RepeatZero);
    let Some(decision) = root.decision() else {
        panic!("program repetition must own a decision");
    };
    assert_eq!(decision.kind(), DecisionKind::Repeat0);
    assert_eq!(decision.arm_count(), 2);
}

#[test]
fn fn_decl_opens_with_the_ordinary_fn_terminal() {
    let root = grammar_node(Production::FnDecl.root()).expect("fn_decl root");
    assert_eq!(root.kind(), GrammarNodeKind::Sequence);
    let first = grammar_node(root.children()[0]).expect("first fn_decl child");
    assert_eq!(first.kind(), GrammarNodeKind::TerminalSequence);
    assert_eq!(
        first.terminals(),
        &[LookaheadPredicate::Terminal(TerminalPredicate::Fixed(
            FixedTerminal::Fn
        ))]
    );
}

#[test]
fn diagnostic_order_contains_no_source_end() {
    assert!(
        diagnostic_terminal_order()
            .iter()
            .all(|item| !matches!(item, LookaheadPredicate::SourceEnd))
    );
}

fn overlaps(left: LookaheadPredicate, right: LookaheadPredicate) -> bool {
    if left == right {
        return true;
    }
    matches!(
        (left, right),
        (
            LookaheadPredicate::Terminal(TerminalPredicate::Fixed(FixedTerminal::Unit)),
            LookaheadPredicate::Terminal(TerminalPredicate::Literal)
        ) | (
            LookaheadPredicate::Terminal(TerminalPredicate::Literal),
            LookaheadPredicate::Terminal(TerminalPredicate::Fixed(FixedTerminal::Unit))
        )
    )
}

#[test]
fn all_detailed_rows_retain_provenance_and_remain_cross_arm_disjoint() {
    assert_eq!(DECISIONS.len(), 166);
    let mut total_rows = 0_usize;
    let mut saw_atom_only = false;
    for decision in &DECISIONS {
        total_rows += decision.rows().len();
        for row in decision.rows() {
            for position in 0..2 {
                let Some(atom) = row.position(position) else {
                    panic!("every row has exactly two atoms");
                };
                match atom.predicate() {
                    LookaheadPredicate::Terminal(_) => assert!(atom.provenance().is_some()),
                    LookaheadPredicate::SourceEnd => assert!(atom.provenance().is_none()),
                }
                saw_atom_only |= atom.is_atom_only();
            }
        }
        for (left_index, left) in decision.rows().iter().enumerate() {
            for right in &decision.rows()[left_index + 1..] {
                if left.arm() == right.arm() {
                    continue;
                }
                let first_overlaps = overlaps(
                    left.position(0)
                        .map(|atom| atom.predicate())
                        .unwrap_or(LookaheadPredicate::SourceEnd),
                    right
                        .position(0)
                        .map(|atom| atom.predicate())
                        .unwrap_or(LookaheadPredicate::SourceEnd),
                );
                let second_overlaps = overlaps(
                    left.position(1)
                        .map(|atom| atom.predicate())
                        .unwrap_or(LookaheadPredicate::SourceEnd),
                    right
                        .position(1)
                        .map(|atom| atom.predicate())
                        .unwrap_or(LookaheadPredicate::SourceEnd),
                );
                assert!(!(first_overlaps && second_overlaps));
            }
        }
    }
    // Count the complete inventory independently by summing each decision's
    // rows, including the explicit interface import arm [FN-3].
    assert_eq!(total_rows, 7_340);
    assert!(saw_atom_only);
}

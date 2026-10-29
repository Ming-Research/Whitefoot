#![allow(clippy::panic)]

use super::{
    DecisionKind, GrammarNodeKind, LookaheadPredicate, Production, diagnostic_terminal_order,
    grammar_node, productions,
};
use crate::syntax::terminal::{
    ALL_FIXED_TERMINALS, ALL_TERMINAL_PREDICATES, FixedTerminal, TerminalPredicate,
};

use super::generated::{DECISIONS, SELECT_ROWS};

#[test]
fn atom_only_provenance_matches_the_gram9_occurrence_table() {
    let expected = [
        (Production::AtomList, 2),
        (Production::Fieldinit, 1),
        (Production::InfixTail, 1),
        (Production::Psuffix, 1),
        (Production::RangeTail, 1),
        (Production::ForBinding, 2),
        (Production::RangeBinder, 2),
    ];
    let mut marked = 0;
    for &production in productions() {
        let count = expected
            .iter()
            .find_map(|&(owner, count)| (owner == production).then_some(count))
            .unwrap_or(0);
        let mut atom_references = 0;
        let mut nodes = vec![production.root()];
        while let Some(id) = nodes.pop() {
            let node = grammar_node(id).expect("generated source node");
            // References are leaves of their owner's source EBNF; following
            // a production root would count another owner's occurrences.
            if node.kind() == GrammarNodeKind::Production(Production::Atom) {
                atom_references += 1;
                assert_eq!(node.is_atom_only_reference(), count != 0, "{production:?}");
                marked += usize::from(node.is_atom_only_reference());
            } else {
                assert!(!node.is_atom_only_reference(), "{production:?}: {node:?}");
            }
            nodes.extend_from_slice(node.children());
        }
        if count != 0 {
            assert_eq!(atom_references, count, "{production:?}");
        }
    }
    // `expr` and `affine_factor` both admit calls as alternatives, so their
    // atom references contribute none of the ten GRAM-9 occurrences.
    assert_eq!(marked, 10);
}

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
    // v0.84's range facts add `apart_clause` [GRAM-4] and `range_clause`,
    // `range_binder` and `range_relation` [GRAM-5]: four productions.
    // v0.90's package bindings add `package_decl` [GRAM-2, MOD-11].
    // v0.142 adds `range_variant` [GRAM-5, RANGE-1]; a guard is a
    // `range_relation`, so no production of its own remains.
    // The diagnostic-only graph_segment and HOLD-1 held_clause each add one
    // production to the 101-production inventory.
    assert_eq!(productions().len(), 103);
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
    // repeat [GRAM-4, SHARE-2]: two decisions. v0.86's entry bindings added
    // the repeat of `"," IDENT "=" "&" place` after the binder [SHARE-2],
    // one decision, and GRAM-4's one atomic target retires it, so every later
    // decision moves back by one.
    // v0.82's `type_invariant*` repeat closes `struct_decl` [GRAM-2, TYPE-11]:
    // one decision, which moves every later decision by one.
    // v0.84's range facts [RANGE-1, RANGE-4, RANGE-5] add ten decisions:
    // the range alternation of `requires_clause`, of `ensures_clause` and of
    // `header_invariant`,
    // `for_stmt`'s optional `apart_clause`, `use_premise`'s optional
    // argument list, `apart_clause`'s `proof_use` repeat, and in
    // `range_clause` the binder repeat, the optional `when` list, its repeat
    // and the conclusion repeat. `apart` and `forall` are two new terminals.
    // v0.90's `package_decl*` repeat opens `graph_file` [GRAM-2, MOD-11]:
    // one decision, and `package` is one new terminal; the IDENT root joins
    // the existing root alternations of `alias_decl` and `module_path`.
    // TYPE-11 adds the named-relation or range-clause choice in
    // `type_invariant`: IDENT "(" and "(" IDENT are disjoint.
    // v0.128 admits projections inside `entry` [GRAM-5, MSR-3]:
    // `pbase` gains one `psuffix*` repeat.
    // v0.128's range guard optional comparison adds one decision.
    // v0.136's unchanged suffix adds a choice inside `clause_expr`.
    // v0.142 shares the comparison, variant or unchanged choice between a
    // guard and a conclusion, adds `range_variant`'s optional `not`, and
    // retires the guard's optional comparison and separate suffix choice.
    // graph_segment adds a repeat and a choice to 182 decisions; held_clause
    // adds its identifier repeat; v0.150 adds one arm-label choice among a
    // variant pattern, literal, named const and `else` [GRAM-4] and no
    // production. GRAM-4's one atomic target retires the target repeat.
    assert_eq!(DECISIONS.len(), 185);
    // `continue` adds the same statement prefixes and followers as `break`:
    // 72 rows at existing decisions and 29 at its optional label decision.
    // Together these extend the previous 7,816-row inventory by 101.
    // The type-invariant choice adds two rows. Its new `invariant (`
    // prefix adds one row each at struct_decl's doc optional, field repeat
    // and type-invariant repeat. The range_clause conclusion repeat gains
    // two exit rows with the type_invariant semicolon's provenance:
    // `; invariant` and `; }`. Together these add seven rows to 7,917.
    // v0.127 makes the struct-destructuring move marker optional: one
    // decision with two move-entry rows and six bare-place exit rows.
    // The entry repeat has 12 entry rows and 80 exit rows. Its closing
    // parenthesis also adds one continuation row each at `place`'s suffix
    // repeat, `psuffix`'s choice and its optional range tail: 95 rows.
    // v0.128's optional guard comparison has six operator starts followed
    // by thirteen affine-expression entry origins, and two guard-list
    // continuations each followed by those thirteen origins: 104 rows.
    // Together the projected entry former and the guard comparison add
    // 95 + 104 rows to the 7,932-row v0.127 inventory.
    // The row inventory preserves FIRST(2), FOLLOW(2) and SELECT provenance.
    // v0.140's `unchanged` conclusion and v0.142's single `range_relation`
    // production (guards and conclusions alike) together give 8,384 rows.
    // graph_segment adds 8 choice rows and 9 repetition rows. Its
    // continuation adds package/pkg/std/IDENT after an entry semicolon,
    // four rows each in entry_decl's final choice and module_path's suffix
    // repetition. These graph-only changes add 25 to main's 8,384 rows.
    // GRAM-9 leaves affine_factor's atom alternative unmarked. Its five
    // atom starts now differ from infix_tail's marked starts after each of
    // six compare_op tokens: 30 rows at compare_op and 30 exit rows at
    // place's psuffix repeat. Other frontiers either truncate that second
    // token or carry only affine continuations, adding 60 rows to 8,409.
    // v0.150's integer arm alternatives add 28 provenance-bearing rows; they
    // share no frontier with graph_segment or the atom-only positions.
    // HOLD-1 adds `requires held` to contract_block's define-repeat exit
    // and requires-repeat entry, and `held (` to requires_clause's choice.
    // The new held_clause repeat contributes `, IDENT` and `) ;`: five
    // additional rows. All old pairs survive unchanged under projection.
    // GRAM-4's one atomic target retires the target repeat and its 40 rows.
    assert_eq!(SELECT_ROWS.len(), 8_462);
    let held = LookaheadPredicate::Terminal(TerminalPredicate::Fixed(FixedTerminal::Held));
    let old_rows = DECISIONS
        .iter()
        .filter(|decision| decision.production() != Production::HeldClause)
        .flat_map(|decision| decision.rows())
        .filter(|row| {
            (0..2).all(|position| {
                row.position(position)
                    .is_none_or(|atom| atom.predicate() != held)
            })
        })
        .count();
    // The rows before HOLD-1 less the 40 the one atomic target retires.
    assert_eq!(old_rows, 8_457);
    let group = DECISIONS
        .iter()
        .find(|decision| decision.production() == Production::HeldClause)
        .expect("the held identifier repeat");
    assert_eq!(group.kind(), DecisionKind::Repeat0);
    assert_eq!(group.rows().len(), 2);
    assert_eq!(
        group.rows()[0].position(0).expect("repeat entry").predicate(),
        LookaheadPredicate::Terminal(TerminalPredicate::Fixed(FixedTerminal::Comma))
    );
    assert_eq!(
        group.rows()[1].position(0).expect("repeat exit").predicate(),
        LookaheadPredicate::Terminal(TerminalPredicate::Fixed(FixedTerminal::RightParen))
    );
    assert_eq!(
        group.rows()[0]
            .position(1)
            .expect("repeat identifier")
            .predicate(),
        LookaheadPredicate::Terminal(TerminalPredicate::Identifier)
    );
    assert_eq!(
        group.rows()[1]
            .position(1)
            .expect("requirement terminator")
            .predicate(),
        LookaheadPredicate::Terminal(TerminalPredicate::Fixed(FixedTerminal::Semicolon))
    );
    // `held` joins v0.136's `unchanged` and v0.142's `not` fixed terminals.
    assert_eq!(diagnostic_terminal_order().len(), 118);
    assert_eq!(productions()[0], Production::Program);
    // v0.70 [GRAM-2] adds the file alias header as an `item` arm and closes
    // with the module graph productions; [GRAM-3] adds `type_path` and
    // [GRAM-5] `callee_path`, so definition positions after each move.
    assert_eq!(productions()[1], Production::Item);
    assert_eq!(productions()[2], Production::AliasDecl);
    assert_eq!(productions()[3], Production::HeapDecl);
    assert_eq!(productions()[13], Production::ContractDefine);
    assert_eq!(productions()[14], Production::RequiresClause);
    assert_eq!(productions()[15], Production::HeldClause);
    assert_eq!(productions()[16], Production::EnsuresClause);
    assert_eq!(productions()[17], Production::ResultRoute);
    assert_eq!(productions()[28], Production::CapabilityBound);
    assert_eq!(productions()[31], Production::GraphFile);
    assert_eq!(productions()[32], Production::GraphSegment);
    assert_eq!(productions()[33], Production::PackageDecl);
    assert_eq!(productions()[36], Production::EntryDecl);
    assert_eq!(productions()[38], Production::TypePath);
    assert_eq!(productions()[52], Production::ForStmt);
    assert_eq!(productions()[53], Production::ForBinding);
    assert_eq!(productions()[54], Production::HeaderInvariant);
    assert_eq!(productions()[55], Production::InvariantStmt);
    assert_eq!(productions()[56], Production::TypeInvariant);
    assert_eq!(productions()[57], Production::ProofUse);
    assert_eq!(productions()[58], Production::UsePremise);
    assert_eq!(productions()[72], Production::ApartClause);
    assert_eq!(productions()[76], Production::CompareOp);
    assert_eq!(productions()[80], Production::CalleePath);
    assert_eq!(productions()[85], Production::ClauseExpr);
    assert_eq!(productions()[86], Production::ClauseOp);
    assert_eq!(productions()[90], Production::RangeTail);
    assert_eq!(productions()[91], Production::RangeClause);
    assert_eq!(productions()[92], Production::RangeBinder);
    assert_eq!(productions()[93], Production::RangeRelation);
    assert_eq!(productions()[94], Production::RangeVariant);
    assert_eq!(productions()[98], Production::Effect);
    assert_eq!(productions()[99], Production::EffectPath);
    assert_eq!(productions()[100], Production::Epbase);
    assert_eq!(productions()[101], Production::Epsuffix);
    assert_eq!(productions()[102], Production::Erange);
    // Above, `productions()` follows first definition in the specification.
    // Below, `index()` follows the generator's independent `ENUM_ORDER`:
    // RangeVariant is definition 94 but dense index 100; GraphSegment is
    // definition 32 but dense index 101; HeldClause is definition 15 but
    // dense index 102.
    assert_eq!(Production::ForStmt.index(), 63);
    assert_eq!(Production::ForBinding.index(), 64);
    assert_eq!(Production::HeaderInvariant.index(), 65);
    assert_eq!(Production::RequiresClause.index(), 66);
    assert_eq!(Production::EnsuresClause.index(), 67);
    assert_eq!(Production::ResultRoute.index(), 68);
    assert_eq!(Production::EffectPath.index(), 69);
    assert_eq!(Production::InvariantStmt.index(), 70);
    assert_eq!(Production::AffineExpr.index(), 71);
    assert_eq!(Production::AffineTerm.index(), 72);
    assert_eq!(Production::AffineFactor.index(), 73);
    assert_eq!(Production::AffineAddOp.index(), 74);
    assert_eq!(Production::ProofUse.index(), 75);
    assert_eq!(Production::ClauseExpr.index(), 77);
    assert_eq!(Production::ClauseOp.index(), 78);
    assert_eq!(Production::CapabilityBound.index(), 79);
    // Retiring `mode` removes its dense slot; surviving productions keep
    // their relative order, including the appended grammar extensions.
    assert_eq!(Production::HeapDecl.index(), 80);
    assert_eq!(Production::RangeTail.index(), 81);
    assert_eq!(Production::Epbase.index(), 82);
    assert_eq!(Production::Epsuffix.index(), 83);
    assert_eq!(Production::Erange.index(), 84);
    assert_eq!(Production::UsePremise.index(), 85);
    // The v0.70 module productions append after every earlier dense slot.
    assert_eq!(Production::AliasDecl.index(), 86);
    assert_eq!(Production::GraphFile.index(), 87);
    assert_eq!(Production::ModuleRow.index(), 88);
    assert_eq!(Production::ModulePath.index(), 89);
    assert_eq!(Production::EntryDecl.index(), 90);
    assert_eq!(Production::TypePath.index(), 91);
    assert_eq!(Production::CalleePath.index(), 92);
    assert_eq!(Production::TypeInvariant.index(), 94);
    assert_eq!(Production::PackageDecl.index(), 99);
    assert_eq!(Production::RangeVariant.index(), 100);
    assert_eq!(Production::GraphSegment.index(), 101);
    assert_eq!(Production::HeldClause.index(), 102);
    // Decision indices follow specification-definition order and EBNF
    // preorder, independently of the dense production indices above.
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
    assert_eq!(DECISIONS[33].production(), Production::ContractBlock);
    assert_eq!(DECISIONS[33].kind(), DecisionKind::Repeat0);
    assert_eq!(DECISIONS[34].production(), Production::ContractBlock);
    assert_eq!(DECISIONS[34].kind(), DecisionKind::Repeat0);
    assert_eq!(DECISIONS[35].production(), Production::ContractBlock);
    assert_eq!(DECISIONS[35].kind(), DecisionKind::Repeat0);
    // `requires_clause` chooses a clause, range clause or held clause.
    assert_eq!(DECISIONS[36].production(), Production::RequiresClause);
    assert_eq!(DECISIONS[36].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[37].production(), Production::HeldClause);
    assert_eq!(DECISIONS[37].kind(), DecisionKind::Repeat0);
    assert_eq!(DECISIONS[38].production(), Production::EnsuresClause);
    assert_eq!(DECISIONS[38].kind(), DecisionKind::Optional);
    // v0.84's `ensures_clause` chooses a clause or a range clause [RANGE-1].
    assert_eq!(DECISIONS[39].production(), Production::EnsuresClause);
    assert_eq!(DECISIONS[39].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[40].production(), Production::ResultRoute);
    assert_eq!(DECISIONS[40].kind(), DecisionKind::Optional);
    // Value mode follows a written type; `&` selects either one referent or
    // a range. Both choices belong to `param` [GRAM-2, REF-4].
    assert_eq!(DECISIONS[64].production(), Production::Param);
    assert_eq!(DECISIONS[64].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[65].production(), Production::Param);
    assert_eq!(DECISIONS[65].kind(), DecisionKind::Choice);
    // graph_segment follows graph_file and adds two decisions before
    // every subsequent production; the source-order pins below include it.
    assert_eq!(DECISIONS[69].production(), Production::GraphSegment);
    assert_eq!(DECISIONS[69].kind(), DecisionKind::Repeat0);
    assert_eq!(DECISIONS[70].production(), Production::GraphSegment);
    assert_eq!(DECISIONS[70].kind(), DecisionKind::Choice);
    // v0.90's `package_decl*` repeat [GRAM-2, MOD-11] precedes every
    // decision from `graph_file` on, which moves each of them by one.
    // `type` keeps its primitive-or-nominal choice, now with the qualified
    // `type_path` arm [GRAM-3, MOD-3], and one `targs?` optional per nominal
    // arm; v0.59's three shape optionals retired with `array`, `box` and
    // `arena`.
    assert_eq!(DECISIONS[76].production(), Production::Type);
    assert_eq!(DECISIONS[76].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[77].production(), Production::Type);
    assert_eq!(DECISIONS[77].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[78].production(), Production::Type);
    assert_eq!(DECISIONS[78].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[100].production(), Production::LoopStmt);
    assert_eq!(DECISIONS[100].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[101].production(), Production::LoopStmt);
    assert_eq!(DECISIONS[101].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[102].production(), Production::LoopStmt);
    assert_eq!(DECISIONS[102].kind(), DecisionKind::Repeat0);
    assert_eq!(DECISIONS[104].production(), Production::ForStmt);
    assert_eq!(DECISIONS[104].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[105].production(), Production::ForStmt);
    assert_eq!(DECISIONS[105].kind(), DecisionKind::Repeat0);
    // v0.84's optional final `apart_clause` [RANGE-5] and the header
    // invariant's affine-or-range choice [RANGE-1].
    assert_eq!(DECISIONS[106].production(), Production::ForStmt);
    assert_eq!(DECISIONS[106].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[108].production(), Production::HeaderInvariant);
    assert_eq!(DECISIONS[108].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[109].production(), Production::InvariantStmt);
    assert_eq!(DECISIONS[109].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[110].production(), Production::InvariantStmt);
    assert_eq!(DECISIONS[110].kind(), DecisionKind::Repeat1);
    assert_eq!(DECISIONS[111].production(), Production::TypeInvariant);
    assert_eq!(DECISIONS[111].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[114].production(), Production::UsePremise);
    assert_eq!(DECISIONS[114].kind(), DecisionKind::Choice);
    // v0.84's written instance `NAME(atom, ...)` [RANGE-4].
    assert_eq!(DECISIONS[115].production(), Production::UsePremise);
    assert_eq!(DECISIONS[115].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[120].production(), Production::BreakStmt);
    assert_eq!(DECISIONS[120].kind(), DecisionKind::Optional);
    assert_eq!(DECISIONS[121].production(), Production::ContinueStmt);
    assert_eq!(DECISIONS[121].kind(), DecisionKind::Optional);
    // v0.150 inserts the arm-label choice [GRAM-4]. Preorder decision
    // numbering shifts every following decision by one.
    assert_eq!(DECISIONS[126].production(), Production::Arm);
    assert_eq!(DECISIONS[126].kind(), DecisionKind::Choice);
    // The call-site marker optional, and v0.82's `atomic_stmt` guard and
    // statement repeat, shift the later place/effect choices; v0.82's
    // retired `for_stmt` marker moves every decision after it back by one,
    // v0.84's range decisions and v0.86's entry bindings move them forward
    // again, and GRAM-4's one atomic target moves them back by one.
    assert_eq!(DECISIONS[139].production(), Production::Call);
    assert_eq!(DECISIONS[139].kind(), DecisionKind::Optional);
    // `psuffix` carries the field, payload and index-or-range choice, and the
    // factored `range_tail?` that keeps the index and range steps
    // strong-LL(2) [GRAM-1, GRAM-5].
    // The projected entry former adds the repeat immediately before these
    // suffix decisions; later decision identities move by one [MSR-3].
    // v0.136's clause suffix choice moves them once more; v0.142 keeps it.
    assert_eq!(DECISIONS[160].production(), Production::Pbase);
    assert_eq!(DECISIONS[160].kind(), DecisionKind::Repeat0);
    // 12 entering rows and 82 exiting rows: replacing range_guard's
    // `unchanged` occurrence with range_variant's `is` occurrence removes one
    // FOLLOW provenance row and adds one, and GRAM-4's one atomic target
    // retires the `,` that followed a target's place, one exiting row.
    // The graph-only diagnostic start contributes no place continuation.
    assert_eq!(DECISIONS[160].rows().len(), 94);
    assert_eq!(DECISIONS[161].production(), Production::Psuffix);
    assert_eq!(DECISIONS[161].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[162].production(), Production::Psuffix);
    assert_eq!(DECISIONS[162].kind(), DecisionKind::Optional);
    // v0.84's `range_clause` [RANGE-1]: the binder repeat, the optional guard
    // list and its repeat, and the conclusion repeat.
    for (index, kind) in [
        (163, DecisionKind::Repeat0),
        (164, DecisionKind::Optional),
        (165, DecisionKind::Repeat0),
        (166, DecisionKind::Repeat0),
    ] {
        assert_eq!(DECISIONS[index].production(), Production::RangeClause);
        assert_eq!(DECISIONS[index].kind(), kind);
    }
    // v0.136's diagnostic-only ordinary unchanged suffix [RANGE-1].
    assert_eq!(DECISIONS[156].production(), Production::ClauseExpr);
    assert_eq!(DECISIONS[156].kind(), DecisionKind::Choice);
    // v0.142 shares comparison, variant and unchanged relations between a
    // guard and a conclusion, then adds the variant's optional `not`.
    // A relation has no bare alternative, so it owns no optional [RANGE-1].
    for (index, production, kind) in [
        (167, Production::RangeRelation, DecisionKind::Choice),
        (168, Production::RangeVariant, DecisionKind::Optional),
    ] {
        assert_eq!(DECISIONS[index].production(), production);
        assert_eq!(DECISIONS[index].kind(), kind);
    }
    // `epsuffix` mirrors it inside an effect row [EFF-1].
    assert_eq!(DECISIONS[183].production(), Production::Epsuffix);
    assert_eq!(DECISIONS[183].kind(), DecisionKind::Choice);
    assert_eq!(DECISIONS[184].production(), Production::Epsuffix);
    assert_eq!(DECISIONS[184].kind(), DecisionKind::Optional);
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
    for (spelling, terminal) in [
        (b"apart".as_slice(), FixedTerminal::Apart),
        (b"forall".as_slice(), FixedTerminal::Forall),
        (b"not".as_slice(), FixedTerminal::Not),
        (b"unchanged".as_slice(), FixedTerminal::Unchanged),
    ] {
        assert_eq!(FixedTerminal::from_spelling(spelling), Some(terminal));
    }
    assert_eq!(ALL_FIXED_TERMINALS.len(), 111);
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
    // The same 185 decisions `complete_inventory_is_pinned` reads out of the
    // generated table, counted a second time by walking every production's
    // node tree. `struct_decl`'s `"opaque"?` optional [GRAM-2, TYPE-2] is
    // reachable from `item`, so the walk and the table agree on it; a
    // decision in the table that no production reaches would show up as the
    // two counts disagreeing.
    assert_eq!(decisions, DECISIONS.len());
    assert_eq!(decisions, 185);
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
    assert_eq!(DECISIONS.len(), 185);
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
    assert_eq!(total_rows, 8_462);
    assert!(saw_atom_only);
}

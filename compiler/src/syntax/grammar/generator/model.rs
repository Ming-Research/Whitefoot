//! Terminal predicates, grammar nodes, and the strong-LL(2) SELECT machinery.

use std::collections::{BTreeMap, BTreeSet};

use super::ebnf::Ast;

// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pred {
    Fixed(&'static str),
    Identifier,
    TypeIdentifier,
    Label,
    OperationName,
    Literal,
    String,
    Digits,
    SourceEnd,
}

impl Pred {
    pub fn rust(self) -> String {
        match self {
            Pred::SourceEnd => "LookaheadPredicate::SourceEnd".to_string(),
            Pred::Fixed(name) => format!(
                "LookaheadPredicate::Terminal(TerminalPredicate::Fixed(FixedTerminal::{name}))"
            ),
            other => format!(
                "LookaheadPredicate::Terminal(TerminalPredicate::{})",
                other.bare()
            ),
        }
    }

    pub fn bare(self) -> &'static str {
        match self {
            Pred::Identifier => "Identifier",
            Pred::TypeIdentifier => "TypeIdentifier",
            Pred::Label => "Label",
            Pred::OperationName => "OperationName",
            Pred::Literal => "Literal",
            Pred::String => "String",
            Pred::Digits => "Digits",
            Pred::Fixed(name) => name,
            Pred::SourceEnd => "SourceEnd",
        }
    }

    /// The DIAG-1 name predicate this terminal is, when it is one.
    pub fn name_predicate(self) -> Option<&'static str> {
        match self {
            Pred::Identifier => Some("Identifier"),
            Pred::TypeIdentifier => Some("TypeIdentifier"),
            Pred::Label => Some("Label"),
            Pred::OperationName => Some("OperationName"),
            _ => None,
        }
    }
}

/// Maps a written terminal spelling to its `FixedTerminal` variant.
pub fn fixed_terminal(spelling: &str) -> Pred {
    const TABLE: &[(&str, &str)] = &[
        ("program", "Program"),
        ("no_heap", "NoHeap"),
        ("struct", "Struct"),
        ("{", "LeftBrace"),
        ("}", "RightBrace"),
        (":", "Colon"),
        (";", "Semicolon"),
        ("enum", "Enum"),
        ("(", "LeftParen"),
        (")", "RightParen"),
        (",", "Comma"),
        ("fn", "Fn"),
        ("->", "ThinArrow"),
        ("requires", "Requires"),
        ("ensures", "Ensures"),
        ("contract", "Contract"),
        ("define", "Define"),
        ("when", "When"),
        ("is", "Is"),
        ("const", "Const"),
        ("=", "Equal"),
        ("doc", "Doc"),
        ("<", "LeftAngle"),
        (">", "RightAngle"),
        ("[", "LeftBracket"),
        ("]", "RightBracket"),
        ("i8", "I8"),
        ("i16", "I16"),
        ("i32", "I32"),
        ("i64", "I64"),
        ("u8", "U8"),
        ("u16", "U16"),
        ("u32", "U32"),
        ("u64", "U64"),
        ("f32", "F32"),
        ("f64", "F64"),
        ("unit", "Unit"),
        ("&", "Ampersand"),
        ("let", "Let"),
        ("propagate", "Propagate"),
        ("set", "Set"),
        ("return", "Return"),
        ("musttail", "Musttail"),
        ("loop", "Loop"),
        ("break", "Break"),
        ("else", "Else"),
        ("give", "Give"),
        ("match", "Match"),
        ("=>", "FatArrow"),
        ("move", "Move"),
        ("deref", "Deref"),
        ("entry", "Entry"),
        (".", "Dot"),
        ("pure", "Pure"),
        ("reads", "Reads"),
        ("writes", "Writes"),
        // FLOOR-5 additions: `if` plus the twenty `infix_op` spellings.
        // `else` already exists for statement and value conditionals. Verified
        // against the fixed delta's [GRAM-5] block, not guessed.
        ("if", "If"),
        ("+", "Plus"),
        ("+wrap", "PlusWrap"),
        ("+defined", "PlusDefined"),
        ("+checked", "PlusChecked"),
        ("+sat", "PlusSat"),
        ("-", "Minus"),
        ("-wrap", "MinusWrap"),
        ("-defined", "MinusDefined"),
        ("-checked", "MinusChecked"),
        ("-sat", "MinusSat"),
        ("*", "Star"),
        ("*wrap", "StarWrap"),
        ("*defined", "StarDefined"),
        ("*checked", "StarChecked"),
        ("*sat", "StarSat"),
        ("/", "Slash"),
        ("/defined", "SlashDefined"),
        ("/checked", "SlashChecked"),
        ("%", "Percent"),
        ("%defined", "PercentDefined"),
        ("%checked", "PercentChecked"),
        ("for", "For"),
        ("invariant", "Invariant"),
        ("use", "Use"),
        ("in", "In"),
        ("..", "DotDot"),
        // v0.41: the six `compare_op` spellings (`<` and `>` already exist)
        // and the call-site type-application delimiter.
        ("==", "EqualEqual"),
        ("!=", "BangEqual"),
        ("<=", "LessEqual"),
        (">=", "GreaterEqual"),
        ("::", "ColonColon"),
        // [GRAM-2, OWN-1, PROV-6]: the declaration modifiers and the
        // capability bound alternatives. `linear` and `affine` name classes in
        // prose only and are ordinary identifiers; [OP-14] `free_empty` is an
        // ordinary [PRE-1] call, not an atom.
        ("opaque", "Opaque"),
        ("nocopy", "Nocopy"),
        ("nodrop", "Nodrop"),
        ("copy", "Copy"),
        ("drop", "Drop"),
        // v0.48 [PRF-1]: the cited-premise multiplicity atom.
        ("times", "Times"),
        ("interface", "Interface"),
        ("binding", "Binding"),
        // x1 [GRAM-2, TYPE-2]: the field modifier that makes a field never a
        // write target.
        ("readonly", "Readonly"),
        // v0.70 [GRAM-2, MOD-1]: the module interface marker, the file alias
        // header and the package root qualifier.
        ("public", "Public"),
        ("alias", "Alias"),
        ("pkg", "Pkg"),
        // v0.72 [MOD-10]: the standard library qualifier.
        ("std", "Std"),
    ];
    if spelling == "[0-9]+" {
        return Pred::Digits;
    }
    for (text, variant) in TABLE {
        if *text == spelling {
            return Pred::Fixed(variant);
        }
    }
    panic!("no FixedTerminal for spelling {spelling:?}");
}

/// Bare EBNF names that denote a terminal predicate rather than a production.
pub fn bare_terminal(name: &str) -> Option<Pred> {
    match name {
        "IDENT" => Some(Pred::Identifier),
        "TYPEID" => Some(Pred::TypeIdentifier),
        "LABEL" => Some(Pred::Label),
        "OPNAME" => Some(Pred::OperationName),
        "STRING" => Some(Pred::String),
        "literal" => Some(Pred::Literal),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Node arena
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Terminal(Vec<Pred>),
    Production(usize),
    Sequence,
    Choice,
    Group,
    Optional,
    RepeatZero,
    RepeatOne,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    pub children: Vec<usize>,
    pub decision: Option<usize>,
    pub atom_only: bool,
}

pub struct Grammar {
    pub nodes: Vec<Node>,
    /// Node id per decision slot, in decision-index order.
    pub decision_order: Vec<usize>,
    /// Root node per production, in specification-definition order.
    pub roots: Vec<usize>,
    pub index: BTreeMap<String, usize>,
}

impl Grammar {
    pub fn build(trees: &[Ast], names: &[String], index: &BTreeMap<String, usize>) -> Self {
        let mut grammar = Grammar {
            nodes: Vec::new(),
            decision_order: Vec::new(),
            roots: vec![usize::MAX; trees.len()],
            index: index.clone(),
        };
        for (position, tree) in trees.iter().enumerate() {
            let owner = names[position].as_str();
            let root = grammar.lower(tree, owner);
            grammar.roots[position] = root;
        }
        grammar
    }

    /// Assigns decision slots: historical slots first, then new ones in node
    /// order.
    pub fn assign_decisions(&mut self, historical: &[usize]) {
        let mut order: Vec<usize> = historical
            .iter()
            .copied()
            .filter(|id| {
                self.nodes.get(*id).is_some_and(|node| {
                    matches!(
                        node.kind,
                        Kind::Choice | Kind::Optional | Kind::RepeatZero | Kind::RepeatOne
                    )
                })
            })
            .collect();
        for (id, node) in self.nodes.iter().enumerate() {
            if matches!(
                node.kind,
                Kind::Choice | Kind::Optional | Kind::RepeatZero | Kind::RepeatOne
            ) && !order.contains(&id)
            {
                order.push(id);
            }
        }
        for (slot, id) in order.iter().enumerate() {
            self.nodes[*id].decision = Some(slot);
        }
        self.decision_order = order;
    }

    fn push(&mut self) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind: Kind::Sequence,
            children: Vec::new(),
            decision: None,
            atom_only: false,
        });
        id
    }

    fn lower(&mut self, ast: &Ast, owner: &str) -> usize {
        let id = self.push();
        let (kind, children, atom_only) = match ast {
            Ast::Terminal(spellings) => {
                let preds = spellings
                    .iter()
                    .map(|spelling| fixed_terminal(spelling))
                    .collect();
                (Kind::Terminal(preds), Vec::new(), false)
            }
            Ast::Reference(name) => {
                if let Some(pred) = bare_terminal(name) {
                    (Kind::Terminal(vec![pred]), Vec::new(), false)
                } else {
                    let target = *self
                        .index
                        .get(name)
                        .unwrap_or_else(|| panic!("unknown production reference {name}"));
                    // GRAM-9: every `atom` reference outside `expr` is an
                    // atom-only position.
                    let atom_only = name == "atom" && owner != "expr";
                    (Kind::Production(target), Vec::new(), atom_only)
                }
            }
            Ast::Sequence(items) => {
                let kids = items
                    .iter()
                    .map(|item| self.lower(item, owner))
                    .collect::<Vec<_>>();
                (Kind::Sequence, kids, false)
            }
            Ast::Choice(arms) => {
                let kids = arms
                    .iter()
                    .map(|arm| self.lower(arm, owner))
                    .collect::<Vec<_>>();
                (Kind::Choice, kids, false)
            }
            Ast::Group(inner) => {
                let kid = self.lower(inner, owner);
                (Kind::Group, vec![kid], false)
            }
            Ast::Optional(inner) => {
                let kid = self.lower(inner, owner);
                (Kind::Optional, vec![kid], false)
            }
            Ast::RepeatZero(inner) => {
                let kid = self.lower(inner, owner);
                (Kind::RepeatZero, vec![kid], false)
            }
            Ast::RepeatOne(inner) => {
                let kid = self.lower(inner, owner);
                (Kind::RepeatOne, vec![kid], false)
            }
        };
        self.nodes[id].kind = kind;
        self.nodes[id].children = children;
        self.nodes[id].atom_only = atom_only;
        id
    }
}

// ---------------------------------------------------------------------------
// Words with provenance
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tok {
    pub pred: Pred,
    pub prov: Option<usize>,
    pub tname: Option<&'static str>,
    pub atom_only: bool,
    pub inside: bool,
}

pub type Word = Vec<Tok>;
pub type WordSet = BTreeSet<Word>;

fn truncate(mut word: Word) -> Word {
    word.truncate(2);
    word
}

/// Two-truncated concatenation of word sets.
pub fn concat(left: &WordSet, right: &WordSet) -> WordSet {
    let mut out = WordSet::new();
    for first in left {
        if first.len() >= 2 {
            out.insert(first.clone());
            continue;
        }
        for second in right {
            let mut joined = first.clone();
            joined.extend_from_slice(second);
            out.insert(truncate(joined));
        }
    }
    out
}

fn clear_names(set: &WordSet) -> WordSet {
    set.iter()
        .map(|word| {
            word.iter()
                .map(|token| Tok {
                    tname: None,
                    ..*token
                })
                .collect()
        })
        .collect()
}

fn mark_atom_only(set: &WordSet) -> WordSet {
    set.iter()
        .map(|word| {
            word.iter()
                .map(|token| Tok {
                    atom_only: true,
                    ..*token
                })
                .collect()
        })
        .collect()
}

pub fn mark_outside(set: &WordSet) -> WordSet {
    set.iter()
        .map(|word| {
            word.iter()
                .map(|token| Tok {
                    inside: false,
                    ..*token
                })
                .collect()
        })
        .collect()
}

/// FIRST(2) per node and the repetition-tail set per repeat node.
pub struct First {
    pub node: Vec<WordSet>,
    /// For a repeat node, the set of zero-or-more further iterations.
    pub tail: Vec<WordSet>,
}

pub fn first_sets(grammar: &Grammar) -> First {
    let count = grammar.nodes.len();
    let mut first = First {
        node: vec![WordSet::new(); count],
        tail: vec![WordSet::new(); count],
    };
    let empty: Word = Vec::new();
    loop {
        let mut changed = false;
        for id in (0..count).rev() {
            let node = &grammar.nodes[id];
            let computed = match &node.kind {
                Kind::Terminal(preds) => {
                    let word: Word = preds
                        .iter()
                        .map(|pred| Tok {
                            pred: *pred,
                            prov: Some(id),
                            tname: pred.name_predicate(),
                            atom_only: false,
                            inside: true,
                        })
                        .collect();
                    let mut set = WordSet::new();
                    set.insert(truncate(word));
                    set
                }
                Kind::Production(target) => {
                    let root = grammar.roots[*target];
                    if node.atom_only {
                        mark_atom_only(&first.node[root])
                    } else {
                        first.node[root].clone()
                    }
                }
                Kind::Sequence => {
                    let mut set = WordSet::new();
                    set.insert(empty.clone());
                    for child in &node.children {
                        set = concat(&set, &first.node[*child]);
                    }
                    set
                }
                Kind::Choice => {
                    let mut set = WordSet::new();
                    for child in &node.children {
                        set.extend(first.node[*child].iter().cloned());
                    }
                    clear_names(&set)
                }
                Kind::Group => first.node[node.children[0]].clone(),
                Kind::Optional => {
                    let mut set = first.node[node.children[0]].clone();
                    set.insert(empty.clone());
                    set
                }
                Kind::RepeatZero | Kind::RepeatOne => {
                    // tail = zero or more further iterations.
                    let mut tail = WordSet::new();
                    tail.insert(empty.clone());
                    let inner = &first.node[node.children[0]];
                    tail.extend(concat(inner, &first.tail[id]));
                    if first.tail[id] != tail {
                        first.tail[id] = tail.clone();
                        changed = true;
                    }
                    if matches!(node.kind, Kind::RepeatZero) {
                        tail
                    } else {
                        concat(inner, &tail)
                    }
                }
            };
            if first.node[id] != computed {
                first.node[id] = computed;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    first
}

/// The continuation set after each node, and after each production.
pub struct Follow {
    pub node: Vec<WordSet>,
    pub production: Vec<WordSet>,
}

pub fn follow_sets(grammar: &Grammar, first: &First, starts: &[usize]) -> Follow {
    let count = grammar.nodes.len();
    let mut follow = Follow {
        node: vec![WordSet::new(); count],
        production: vec![WordSet::new(); grammar.roots.len()],
    };
    // Each start production is followed by end of source: `program` for
    // interface and implementation sources, `graph_file` for the module graph.
    for start in starts {
        let mut initial = WordSet::new();
        initial.insert(Vec::new());
        follow.production[*start] = initial;
    }
    // PRE-1 supplies ordinary signature records independently of writer
    // items. Their record terminator is a semicolon followed by source end;
    // the same fn_sig production checks the declaration itself. This root
    // continuation does not add fn_sig to the writer's item production.
    if let Some(signature) = grammar.index.get("fn_sig") {
        // The record uses the same separator as an ordinary formal member.
        // Retain that grammar occurrence as the lookahead's provenance.
        let formal = grammar.index["interface_decl"];
        let mut pending = vec![grammar.roots[formal]];
        let mut separator = None;
        while let Some(node) = pending.pop() {
            if matches!(&grammar.nodes[node].kind, Kind::Terminal(predicates)
                if predicates.as_slice() == [Pred::Fixed("Semicolon")])
            {
                separator = Some(node);
                break;
            }
            pending.extend_from_slice(&grammar.nodes[node].children);
        }
        follow.production[*signature].insert(vec![Tok {
            pred: Pred::Fixed("Semicolon"),
            prov: Some(separator.expect("formal members have a signature separator")),
            tname: None,
            atom_only: false,
            inside: false,
        }]);
    }

    loop {
        let mut changed = false;
        // Push production-level continuations down to roots.
        for (production, root) in grammar.roots.iter().enumerate() {
            let value = follow.production[production].clone();
            if follow.node[*root] != value {
                let merged: WordSet = follow.node[*root].union(&value).cloned().collect();
                if follow.node[*root] != merged {
                    follow.node[*root] = merged;
                    changed = true;
                }
            }
        }
        for id in 0..count {
            let node = &grammar.nodes[id];
            let outer = follow.node[id].clone();
            match &node.kind {
                Kind::Sequence => {
                    for (position, child) in node.children.iter().enumerate() {
                        let mut set = outer.clone();
                        for later in node.children[position + 1..].iter().rev() {
                            set = concat(&first.node[*later], &set);
                        }
                        changed |= merge(&mut follow.node[*child], &set);
                    }
                }
                Kind::Choice | Kind::Group | Kind::Optional => {
                    for child in &node.children {
                        changed |= merge(&mut follow.node[*child], &outer);
                    }
                }
                Kind::RepeatZero | Kind::RepeatOne => {
                    let set = concat(&first.tail[id], &outer);
                    changed |= merge(&mut follow.node[node.children[0]], &set);
                }
                Kind::Production(target) => {
                    let value = outer.clone();
                    changed |= merge(&mut follow.production[*target], &value);
                }
                Kind::Terminal(_) => {}
            }
        }
        if !changed {
            break;
        }
    }
    follow
}

fn merge(into: &mut WordSet, from: &WordSet) -> bool {
    let before = into.len();
    into.extend(from.iter().cloned());
    into.len() != before
}

/// Pads a word to exactly two tokens with the source-end sentinel.
pub fn pad(word: &Word) -> (Tok, Tok) {
    let end = Tok {
        pred: Pred::SourceEnd,
        prov: None,
        tname: None,
        atom_only: false,
        inside: false,
    };
    let first = word.first().copied().unwrap_or(end);
    let second = word.get(1).copied().unwrap_or(end);
    (first, second)
}

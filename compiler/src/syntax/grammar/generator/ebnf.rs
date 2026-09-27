//! Extract and parse the normative EBNF of a Whitefoot kernel specification.
//!
//! Every normative production lives in a fenced block whose info string is
//! `wf-ebnf` followed by the id of the rule that owns it, so extraction keys
//! on the info string and never on a prose anchor. Seven such fences define
//! source productions: GRAM-2..GRAM-5 carry the source grammar, and
//! CONST-1, CONST-2, and EFF-1 carry `const`, `cvalue`, `effects`, and
//! `effect`, which were written inline in prose before v0.30.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Owner {
    Gram2,
    Gram3,
    Gram4,
    Gram5,
    Const1,
    Const2,
    Eff1,
}

impl Owner {
    /// The rule id a `wf-ebnf` fence names in its info string.
    fn from_info(id: &str) -> Option<Self> {
        Some(match id {
            "GRAM-2" => Owner::Gram2,
            "GRAM-3" => Owner::Gram3,
            "GRAM-4" => Owner::Gram4,
            "GRAM-5" => Owner::Gram5,
            "CONST-1" => Owner::Const1,
            "CONST-2" => Owner::Const2,
            "EFF-1" => Owner::Eff1,
            _ => return None,
        })
    }

    pub fn rust(self) -> &'static str {
        match self {
            Owner::Gram2 => "RuleOwner::Gram2",
            Owner::Gram3 => "RuleOwner::Gram3",
            Owner::Gram4 => "RuleOwner::Gram4",
            Owner::Gram5 => "RuleOwner::Gram5",
            Owner::Const1 => "RuleOwner::Const1",
            Owner::Const2 => "RuleOwner::Const2",
            Owner::Eff1 => "RuleOwner::Eff1",
        }
    }
}

/// One raw production: name, right-hand side text, and the rule that owns it.
#[derive(Clone, Debug)]
pub struct RawProduction {
    pub name: String,
    pub body: String,
    pub owner: Owner,
}

/// Every `wf-ebnf` fence body with its owner, in specification order.
fn ebnf_blocks(text: &str) -> Vec<(Owner, String)> {
    let mut blocks = Vec::new();
    let mut open: Option<(Owner, Vec<&str>)> = None;
    for line in text.lines() {
        match open.as_mut() {
            Some((_, body)) => {
                if line == "```" {
                    let (owner, body) = open.take().expect("an open fence");
                    blocks.push((owner, body.join("\n")));
                } else {
                    body.push(line);
                }
            }
            None => {
                if let Some(info) = line.strip_prefix("```wf-ebnf ") {
                    let owner = Owner::from_info(info.trim())
                        .unwrap_or_else(|| panic!("unknown wf-ebnf owner {info}"));
                    open = Some((owner, Vec::new()));
                }
            }
        }
    }
    assert!(open.is_none(), "a wf-ebnf fence is unterminated");
    blocks
}

/// Splits a block of `name := body` lines, honouring continuation lines.
fn split_block(block: &str, owner: Owner, out: &mut Vec<RawProduction>) {
    let mut current: Option<RawProduction> = None;
    for line in block.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // A definition line carries `:=` before any `"`-quoted terminal.
        let define = line.find(":=").filter(|position| {
            line[..*position]
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c.is_whitespace())
        });
        match define {
            Some(position) => {
                if let Some(previous) = current.take() {
                    out.push(previous);
                }
                current = Some(RawProduction {
                    name: line[..position].trim().to_string(),
                    body: line[position + 2..].trim().to_string(),
                    owner,
                });
            }
            None => {
                let entry = current.as_mut().expect("continuation without a definition");
                entry.body.push(' ');
                entry.body.push_str(line.trim());
            }
        }
    }
    if let Some(previous) = current.take() {
        out.push(previous);
    }
}

/// Every normative production in specification-definition order.
pub fn productions(spec: &str) -> Vec<RawProduction> {
    let mut out = Vec::new();
    let blocks = ebnf_blocks(spec);
    assert_eq!(
        blocks.len(),
        7,
        "the specification has seven wf-ebnf fences"
    );
    for (owner, block) in blocks {
        split_block(&block, owner, &mut out);
    }
    assert_eq!(
        out.len(),
        92,
        "the seven wf-ebnf fences define 92 productions"
    );
    out
}

// ---------------------------------------------------------------------------
// EBNF syntax tree
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Ast {
    /// One source terminal occurrence, as its sequence of raw token spellings.
    ///
    /// v0.60 has no compound fixed atom, so every occurrence is a one-element
    /// vector today; `"&uniq"`, the only atom that ever expanded to two raw
    /// tokens, retired with the permission marker [GRAM-1]. The sequence shape
    /// is retained rather than collapsed to a single `String`: collapsing it
    /// would also have to change `Kind::Terminal`, `GrammarNodeKind::
    /// TerminalSequence`, `GrammarNode::terminals()` and the generated
    /// `GRAMMAR_TERMINALS` arena, which is a refactor with no specification
    /// pressure behind it.
    Terminal(Vec<String>),
    /// A reference to another production, by name.
    Reference(String),
    Sequence(Vec<Ast>),
    Choice(Vec<Ast>),
    Group(Box<Ast>),
    Optional(Box<Ast>),
    RepeatZero(Box<Ast>),
    RepeatOne(Box<Ast>),
}

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
}

impl<'a> Parser<'a> {
    fn skip_space(&mut self) {
        while self.at < self.text.len() && self.text[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_space();
        self.text.get(self.at).copied()
    }

    /// alternation := sequence ("|" sequence)*
    fn alternation(&mut self) -> Ast {
        let mut arms = vec![self.sequence()];
        while self.peek() == Some(b'|') {
            self.at += 1;
            arms.push(self.sequence());
        }
        if arms.len() == 1 {
            arms.pop().expect("one arm")
        } else {
            Ast::Choice(arms)
        }
    }

    fn sequence(&mut self) -> Ast {
        let mut items = Vec::new();
        while let Some(byte) = self.peek() {
            if byte == b'|' || byte == b')' {
                break;
            }
            items.push(self.postfix());
        }
        if items.len() == 1 {
            items.pop().expect("one item")
        } else {
            Ast::Sequence(items)
        }
    }

    fn postfix(&mut self) -> Ast {
        let mut node = self.primary();
        loop {
            match self.text.get(self.at).copied() {
                Some(b'?') => {
                    self.at += 1;
                    node = Ast::Optional(Box::new(node));
                }
                Some(b'*') => {
                    self.at += 1;
                    node = Ast::RepeatZero(Box::new(node));
                }
                Some(b'+') => {
                    self.at += 1;
                    node = Ast::RepeatOne(Box::new(node));
                }
                _ => break,
            }
        }
        node
    }

    fn primary(&mut self) -> Ast {
        let byte = self.peek().expect("primary");
        match byte {
            b'(' => {
                self.at += 1;
                let inner = self.alternation();
                assert_eq!(self.peek(), Some(b')'), "group closes");
                self.at += 1;
                Ast::Group(Box::new(inner))
            }
            b'"' => {
                self.at += 1;
                let start = self.at;
                while self.text[self.at] != b'"' {
                    self.at += 1;
                }
                let spelling = std::str::from_utf8(&self.text[start..self.at])
                    .expect("utf8")
                    .to_string();
                self.at += 1;
                Ast::Terminal(split_spelling(&spelling))
            }
            _ => {
                let start = self.at;
                while self.at < self.text.len()
                    && (self.text[self.at].is_ascii_alphanumeric() || self.text[self.at] == b'_')
                {
                    self.at += 1;
                }
                assert!(self.at > start, "a name has at least one byte");
                let name = std::str::from_utf8(&self.text[start..self.at])
                    .expect("utf8")
                    .to_string();
                Ast::Reference(name)
            }
        }
    }
}

/// The raw formed tokens one written terminal covers [GRAM-1].
///
/// Every v0.60 fixed atom is exactly one raw token, so this is the identity
/// today; see [`Ast::Terminal`] for why the sequence shape is kept.
fn split_spelling(spelling: &str) -> Vec<String> {
    vec![spelling.to_string()]
}

pub fn parse_body(body: &str) -> Ast {
    let mut parser = Parser {
        text: body.as_bytes(),
        at: 0,
    };
    let ast = parser.alternation();
    parser.skip_space();
    assert_eq!(parser.at, parser.text.len(), "body fully consumed: {body}");
    ast
}

/// Parses every production body, keyed by name, preserving definition order.
pub fn parse_all(raw: &[RawProduction]) -> (Vec<Ast>, BTreeMap<String, usize>) {
    let mut index = BTreeMap::new();
    for (position, production) in raw.iter().enumerate() {
        let previous = index.insert(production.name.clone(), position);
        assert!(
            previous.is_none(),
            "duplicate production {}",
            production.name
        );
    }
    let trees = raw.iter().map(|entry| parse_body(&entry.body)).collect();
    (trees, index)
}

//! The program generator.
//!
//! Every form it emits is one production of the [GRAM-4] statement fence or the
//! [GRAM-5] expression fence, chosen under a typing and ownership environment
//! the generator carries: which scalars and buffers are live, which entry
//! inputs exist, which regions are open, how deep the nesting is. That is what
//! "grammar-driven" has to mean for a language whose acceptance is a borrow and
//! effect judgment -- a free derivation over the fence produces well-formed
//! text that is rejected essentially always, and a fuzzer whose programs are
//! rejected tests the parser and nothing else.
//!
//! The environment is what makes the emitted program *canonical* rather than
//! merely parseable: subscripts are either constant against a known buffer
//! length or the binder of a `for` whose upper endpoint is that length,
//! divisors are nonzero literals, every borrow is taken inside a region the
//! generator opened, and every linear value is consumed on each exit. The
//! ordinary entry owns its local state, while helpers declare exact parameter
//! rows and contracts. Acceptance is still verified by the compiler: the
//! campaign counts every rejection by the rule its diagnostic cites, which is
//! how a bias in this file becomes visible instead of silent.
//!
//! The shape weights lean toward what [PAR-1] and [PAR-2] can permit
//! and toward their exact boundaries, because a permission that is never
//! granted and a permission that is wrongly granted are both invisible to a
//! generator that only writes the easy middle.

use std::collections::BTreeSet;

use crate::rng::Rng;

/// The named shapes a program contains. The campaign reports the distribution,
/// so a shape that stopped being generated -- because a compiler change started
/// rejecting it -- shows up as a zero rather than as silence.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Shape {
    Arithmetic,
    Branch,
    CountedLoop,
    AccumulatorLoop,
    UnboundedLoop,
    NestedLoop,
    StdoutWrite,
    StderrWrite,
    BulkWrite,
    IndependentPair,
    SameOutputPair,
    PureCallPair,
    SharedSourcePair,
    ReadThenWriteBuffer,
    FileLoopIterationOwn,
    FileLoopHoistedScratch,
    FileLoopBreakAfterSubmission,
    FileLoopSharedScratch,
    DirectoryScan,
    Invariant,
    GiveMatch,
    SliceView,
    TypedExit,
    ArgumentRead,
}

impl Shape {
    pub fn spelling(self) -> &'static str {
        match self {
            Shape::Arithmetic => "arithmetic",
            Shape::Branch => "branch",
            Shape::CountedLoop => "counted-loop",
            Shape::AccumulatorLoop => "accumulator-loop",
            Shape::UnboundedLoop => "unbounded-loop",
            Shape::NestedLoop => "nested-loop",
            Shape::StdoutWrite => "stdout-write",
            Shape::StderrWrite => "stderr-write",
            Shape::BulkWrite => "bulk-write",
            Shape::IndependentPair => "independent-pair",
            Shape::SameOutputPair => "same-output-pair",
            Shape::PureCallPair => "pure-call-pair",
            Shape::SharedSourcePair => "shared-source-pair",
            Shape::ReadThenWriteBuffer => "read-then-write-buffer",
            Shape::FileLoopIterationOwn => "file-loop-iteration-own",
            Shape::FileLoopHoistedScratch => "file-loop-hoisted-scratch",
            Shape::FileLoopBreakAfterSubmission => "file-loop-break-after-submission",
            Shape::FileLoopSharedScratch => "file-loop-shared-scratch",
            Shape::DirectoryScan => "directory-scan",
            Shape::Invariant => "invariant",
            Shape::GiveMatch => "give-match",
            Shape::SliceView => "slice-view",
            Shape::TypedExit => "typed-exit",
            Shape::ArgumentRead => "argument-read",
        }
    }
}

pub struct Program {
    pub source: String,
    pub shapes: Vec<Shape>,
    /// The program publishes more than one host pipe buffer of bytes, so a
    /// delayed FIFO reader makes its writes genuinely wait rather than merely
    /// arrive late.
    pub bulk_output: bool,
}

/// Text under construction, with the indentation the emitted source carries.
struct Emit {
    text: String,
    indent: usize,
}

impl Emit {
    fn new() -> Self {
        Self {
            text: String::new(),
            indent: 1,
        }
    }

    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.text.push_str("  ");
        }
        self.text.push_str(text);
        self.text.push('\n');
    }

    fn open(&mut self, text: &str) {
        self.line(text);
        self.indent += 1;
    }

    fn close(&mut self) {
        self.indent -= 1;
        self.line("}");
    }
}

/// One live `u64` binding usable as an operand.
type Scalar = String;

/// One live `buffer<u8>` binding and its statically known length, which is what
/// makes a constant subscript provable without a guard.
struct Buffer {
    name: String,
    length: u64,
}

struct Gen {
    rng: Rng,
    body: Emit,
    next_name: u32,
    next_label: u32,
    depth: usize,
    statements: usize,
    limit: usize,

    have_err: bool,
    have_files: bool,
    have_args: bool,

    need_fold: bool,
    need_render: bool,
    need_mix: bool,
    need_view: bool,

    scalars: Vec<Scalar>,
    buffers: Vec<Buffer>,
    shapes: BTreeSet<Shape>,
    bulk_output: bool,
}

pub fn generate(seed: u64) -> Program {
    let mut rng = Rng::new(seed);
    let have_err = rng.chance(55);
    let have_files = rng.chance(65);
    let have_args = rng.chance(30);
    let generator = Gen {
        rng,
        body: Emit::new(),
        next_name: 0,
        next_label: 0,
        depth: 0,
        statements: 0,
        limit: 220,
        have_err,
        have_files,
        have_args,
        need_fold: false,
        need_render: true,
        need_mix: false,
        need_view: false,
        scalars: Vec::new(),
        buffers: Vec::new(),
        shapes: BTreeSet::new(),
        bulk_output: false,
    };
    generator.program()
}

impl Gen {
    fn close_cwd(&mut self) {
        self.body.open("region {");
        self.body
            .line("close_directory(factory: &uniq files, directory: move cwd);");
        self.body.close();
    }

    fn name(&mut self, stem: &str) -> String {
        self.next_name += 1;
        format!("{stem}_{}", self.next_name)
    }

    fn label(&mut self) -> String {
        self.next_label += 1;
        format!("@walk_{}", self.next_label)
    }

    fn budget(&self) -> bool {
        self.statements < self.limit
    }

    fn spend(&mut self) {
        self.statements += 1;
    }

    /// A `u64` atom: a live scalar or a literal. Every operand position the
    /// generator fills is one `atom` of [GRAM-5], never a nested expression,
    /// because the fence admits none.
    fn scalar_atom(&mut self) -> String {
        if self.scalars.is_empty() || self.rng.chance(30) {
            format!("{}_u64", self.rng.between(0, 97))
        } else {
            let index = self.rng.below(self.scalars.len() as u64) as usize;
            self.scalars[index].clone()
        }
    }

    fn byte_literal(&mut self) -> String {
        format!("{}_u8", self.rng.between(32, 126))
    }

    /// A `Bool` expression: one comparison call over two atoms.
    fn condition(&mut self) -> String {
        let left = self.scalar_atom();
        let right = self.scalar_atom();
        let op = *self.rng.pick(&["<", "<=", ">", ">=", "==", "!="]);
        format!("{left} {op} {right}")
    }

    fn program(mut self) -> Program {
        self.body.line("doc \"A generated differential-fuzz program: real I/O, real control flow, one published digest.\";");
        self.body.line("let Inputs(args: args, cwd: cwd, stdout: out, stderr: err, handles: files, stdin: input, ..) = move inputs;");
        self.body.line("let total = 0_u64;");
        self.scalars.push("total".to_owned());

        let blocks = self.rng.between(3, 7);
        for _ in 0..blocks {
            if !self.budget() {
                break;
            }
            self.block();
        }
        self.publish_digest();

        let mut source = String::from(IO_HELPERS);
        if self.need_mix {
            source.push_str(MIX_HELPER);
            source.push('\n');
        }
        if self.need_fold {
            source.push_str(FOLD_HELPER);
            source.push('\n');
        }
        if self.need_view {
            source.push_str(VIEW_HELPER);
            source.push('\n');
        }
        if self.need_render {
            source.push_str(RENDER_HELPER);
            source.push('\n');
        }
        source.push_str(&self.header());
        source.push_str(&self.body.text);
        source.push_str("}\n");

        Program {
            source,
            shapes: self.shapes.iter().copied().collect(),
            bulk_output: self.bulk_output,
        }
    }

    /// The ordinary entry transfers its aggregate into local owners. Opaque
    /// cleanup is explicit; all accesses in this owned body are local.
    fn header(&self) -> String {
        "fn main(inputs: own Inputs) -> status: own ExitStatus pure {\n".to_owned()
    }

    /// Renders the accumulated total as a fixed twenty-digit line and publishes
    /// it, then leaves through an exit status derived from the same total. Both
    /// observables therefore depend on every statement the program executed,
    /// which is what makes byte equality a real oracle rather than a check that
    /// two runs both printed nothing.
    fn publish_digest(&mut self) {
        let line = self.name("digest");
        let cursor = self.name("cursor");
        self.body
            .line(&format!("let {line} = buffer_new(32_u64, 32_u8);"));
        self.body.line(&format!("let {cursor} = 0_u64;"));
        self.body.open("region {");
        self.body.line(&format!(
            "set {cursor} = render_u64(destination: &uniq {line}, at: 0_u64, value: total);"
        ));
        self.body.close();
        self.body.line(&format!("set {line}[20_u64] = 10_u8;"));
        let outcome = self.name("published");
        self.body.open("region {");
        self.body.open(&format!(
            "match publish_bytes(factory: &uniq files, output: &uniq out, source: &{line}, start: 0_u64, end: 21_u64) {{"
        ));
        self.body.open(&format!("Ok(value: {outcome}) => {{"));
        self.body.close();
        let failure = self.name("failed");
        self.body.open(&format!("Err(error: {failure}) => {{"));
        self.body.close();
        self.body.close();
        self.body.close();

        let wide = self.name("code_wide");
        let narrow = self.name("code_narrow");
        let code = self.name("code");
        let exact = self.name("code_exact");
        let failed = self.name("code_failed");
        self.body.line(&format!("let {wide} = total % 251_u64;"));
        self.body
            .line(&format!("let {narrow} = cvt::<u64, u8>({wide});"));
        self.body.open(&format!("let {code} = match {narrow} {{"));
        self.body.open(&format!("Ok(value: {exact}) => {{"));
        self.body.line(&format!("give {exact};"));
        self.body.close();
        self.body.open(&format!("Err(error: {failed}) => {{"));
        self.body.line("give 9_u8;");
        self.body.close();
        self.body.close();
        self.close_cwd();
        self.body
            .line(&format!("return exit_status(code: {code});"));
    }
}

const IO_HELPERS: &str = r#"fn publish_bytes(factory: &uniq HandleFactory, output: &uniq OutputStream, source: &buffer<u8>, start: own u64, end: own u64) -> result: own Result<u64, IoError> reads(factory, output, source), writes(factory, output) contract {
  requires start <= end;
  requires end <= len_of(deref(source));
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
} {
  region {
    let view = slice_of(&deref(source));
    region {
      match write_once(factory: &uniq deref(factory), output: &uniq deref(output), source: &view, start: start, end: end) {
        Ok(value: next) => {
          return Ok<u64, IoError>(value: next);
        }
        Err(error: problem) => {
          return Err<u64, IoError>(error: move problem);
        }
      }
    }
  }
}

fn open_named(factory: &uniq HandleFactory, root: &DirectoryRead, name: &buffer<u8>, start: own u64, end: own u64) -> result: own Result<ReadFile, IoError> reads(factory, root, name), writes(factory) contract {
  requires start <= end;
  requires end <= len_of(deref(name));
} {
  region {
    let view = slice_of(&deref(name));
    region {
      let opened = open_file(factory: &uniq deref(factory), root: root, name: &view, start: start, end: end);
      return move opened;
    }
  }
}

fn read_prefix(factory: &uniq HandleFactory, file: &uniq ReadFile, destination: &uniq buffer<u8>, start: own u64, end: own u64) -> result: own Result<u64, ReadStop> reads(factory, file, destination), writes(factory, file, destination) contract {
  requires start <= end;
  requires end <= len_of(deref(destination));
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
  ensures len_of(deref(destination)) == len_of(deref(entry(destination)));
} {
  region {
    let view = mut_slice_of(&uniq deref(destination));
    region {
      match read_at(factory: &uniq deref(factory), file: &uniq deref(file), destination: &uniq view, file_offset: 0_u64, start: start, end: end) {
        Ok(value: next) => {
          return Ok<u64, ReadStop>(value: next);
        }
        Err(error: problem) => {
          return Err<u64, ReadStop>(error: move problem);
        }
      }
    }
  }
}

fn list_batch(source: &uniq DirectorySource, destination: &uniq buffer<u8>, start: own u64, end: own u64) -> (result: own Result<unit, ListStop>, next: own u64, entries: own u64) reads(source, destination), writes(source, destination) contract {
  requires start <= end;
  requires end <= len_of(deref(destination));
  ensures start <= next;
  ensures next <= end;
  ensures len_of(deref(destination)) == len_of(deref(entry(destination)));
} {
  region {
    let view = mut_slice_of(&uniq deref(destination));
    region {
      let (copied, endpoint, count) = directory_next(source: &uniq deref(source), destination: &uniq view, start: start, end: end);
      return move copied, endpoint, count;
    }
  }
}

"#;

const MIX_HELPER: &str = r#"fn mix(a: own u64, b: own u64) -> result: own u64 pure {
  doc "Mixes two scalars into one with total operations and no effect of any kind.";
  let scaled = a *wrap 2654435761_u64;
  let added = scaled +wrap b;
  let folded = ixor(added, 1099511628211_u64);
  return folded;
}
"#;

const FOLD_HELPER: &str = r#"fn fold_prefix(source: &buffer<u8>, produced: own u64, seed: own u64) -> result: own u64 reads(source) {
  doc "Folds one prefix of a buffer into a running order-sensitive checksum.";
  let room = len_of(deref(source));
  let sum = seed;
  let at = 0_u64;
  loop @fold {
    let scanned = at >= produced;
    if scanned {
      break @fold;
    }
    let readable = at < room;
    if readable {
    } else {
      break @fold;
    }
    let byte = deref(source)[at];
    let widened = cvt::<u8, u64>(byte);
    set sum = sum *wrap 31_u64;
    set sum = sum +wrap widened;
    set at = at +wrap 1_u64;
  }
  return sum;
}
"#;

const VIEW_HELPER: &str = r#"fn fold_view(view: own Slice<u8>, produced: own u64, seed: own u64) -> result: own u64 reads(view) {
  doc "Folds one prefix of a direct view into a running order-sensitive checksum.";
  let room = len_of(view);
  let sum = seed;
  let at = 0_u64;
  loop @scan {
    let scanned = at >= produced;
    if scanned {
      break @scan;
    }
    let readable = at < room;
    if readable {
    } else {
      break @scan;
    }
    let byte = view[at];
    let widened = cvt::<u8, u64>(byte);
    set sum = sum *wrap 31_u64;
    set sum = sum +wrap widened;
    set at = at +wrap 1_u64;
  }
  return sum;
}
"#;

const RENDER_HELPER: &str = r#"fn render_u64(destination: &uniq buffer<u8>, at: own u64, value: own u64) -> result: own u64 reads(destination), writes(destination) contract {
  ensures len_of(deref(destination)) == len_of(deref(entry(destination)));
} {
  doc "Renders one twenty-digit zero-padded decimal number and reports the position after it.";
  let room = len_of(deref(destination));
  let remaining = value;
  let position = at +wrap 20_u64;
  loop @digits {
    let done = position <= at;
    if done {
      break @digits;
    }
    set position = position -wrap 1_u64;
    let digit = remaining % 10_u64;
    let narrowed = cvt::<u64, u8>(digit);
    let byte = match narrowed {
      Ok(value: exact) => {
        give 48_u8 +wrap exact;
      }
      Err(error: failure) => {
        give 48_u8;
      }
    }
    let writable = position < room;
    if writable {
      set deref(destination)[position] = byte;
    }
    set remaining = remaining / 10_u64;
  }
  return at +wrap 20_u64;
}
"#;

/// The shape catalog. Each method emits one complete, self-contained group of
/// statements and leaves the environment consistent: every binding it adds is
/// live, every affine value it creates is consumed, every region it opens is
/// closed, and every subscript it writes is provable from a length the
/// generator knows.
impl Gen {
    fn block(&mut self) {
        let mut options: Vec<(Shape, u64)> = vec![
            (Shape::Arithmetic, 9),
            (Shape::Branch, 9),
            (Shape::CountedLoop, 9),
            (Shape::AccumulatorLoop, 9),
            (Shape::UnboundedLoop, 7),
            (Shape::StdoutWrite, 9),
            (Shape::BulkWrite, 3),
            (Shape::SameOutputPair, 8),
            (Shape::PureCallPair, 7),
            (Shape::Invariant, 7),
            (Shape::GiveMatch, 6),
            (Shape::SliceView, 6),
            (Shape::TypedExit, 3),
        ];
        if self.have_err {
            options.push((Shape::StderrWrite, 8));
            options.push((Shape::IndependentPair, 11));
            options.push((Shape::SharedSourcePair, 6));
        }
        if self.have_files {
            options.push((Shape::FileLoopIterationOwn, 20));
            options.push((Shape::FileLoopHoistedScratch, 6));
            options.push((Shape::FileLoopBreakAfterSubmission, 4));
            options.push((Shape::FileLoopSharedScratch, 4));
            options.push((Shape::ReadThenWriteBuffer, 6));
            options.push((Shape::DirectoryScan, 5));
        }
        if self.have_args {
            options.push((Shape::ArgumentRead, 5));
        }
        let total: u64 = options.iter().map(|(_, weight)| weight).sum();
        let mut draw = self.rng.below(total);
        let mut chosen = options[0].0;
        for (shape, weight) in &options {
            if draw < *weight {
                chosen = *shape;
                break;
            }
            draw -= weight;
        }
        self.shapes.insert(chosen);
        match chosen {
            Shape::Arithmetic => self.arithmetic_block(),
            Shape::Branch => self.branch_block(),
            Shape::CountedLoop => self.counted_loop_block(),
            Shape::AccumulatorLoop => self.accumulator_loop_block(),
            Shape::UnboundedLoop => self.unbounded_loop_block(),
            Shape::StdoutWrite => self.write_block(false),
            Shape::StderrWrite => self.write_block(true),
            Shape::BulkWrite => self.bulk_write_block(),
            Shape::IndependentPair => self.output_pair_block(true, false),
            Shape::SameOutputPair => self.output_pair_block(false, false),
            Shape::SharedSourcePair => self.output_pair_block(true, true),
            Shape::PureCallPair => self.pure_call_pair_block(),
            Shape::ReadThenWriteBuffer => self.read_then_write_block(),
            Shape::FileLoopIterationOwn => self.file_loop_block(FileLoop::IterationOwn),
            Shape::FileLoopHoistedScratch => self.file_loop_block(FileLoop::HoistedScratch),
            Shape::FileLoopBreakAfterSubmission => {
                self.file_loop_block(FileLoop::BreakAfterSubmission)
            }
            Shape::FileLoopSharedScratch => self.file_loop_block(FileLoop::SharedScratch),
            Shape::DirectoryScan => self.directory_block(),
            Shape::Invariant => self.invariant_block(),
            Shape::GiveMatch => self.give_match_block(),
            Shape::SliceView => self.slice_view_block(),
            Shape::TypedExit => self.typed_exit_block(),
            Shape::ArgumentRead => self.argument_block(),
            Shape::NestedLoop => self.counted_loop_block(),
        }
    }

    /// Saves the binding environment across a nested block, because a `let`
    /// inside a block is not in scope after it.
    fn scope<F: FnOnce(&mut Self)>(&mut self, inner: F) {
        let scalars = self.scalars.len();
        let buffers = self.buffers.len();
        self.depth += 1;
        inner(self);
        self.depth -= 1;
        self.scalars.truncate(scalars);
        self.buffers.truncate(buffers);
    }

    fn declare_buffer(&mut self, length: u64, fill: u64) -> String {
        let name = self.name("store");
        self.spend();
        self.body.line(&format!(
            "let {name} = buffer_new({length}_u64, {fill}_u8);"
        ));
        self.buffers.push(Buffer {
            name: name.clone(),
            length,
        });
        name
    }

    /// One fixture file name, `fNN.dat`, written byte by byte at constant
    /// positions of a length-8 buffer, so every subscript is provable and the
    /// name range is exactly seven bytes.
    fn declare_name(&mut self, digit: Option<&str>) -> String {
        let name = self.declare_buffer(8, 0);
        let fixed: u64 = self.rng.below(8);
        self.body.line(&format!("set {name}[0_u64] = 102_u8;"));
        self.body.line(&format!("set {name}[1_u64] = 48_u8;"));
        match digit {
            Some(index) => {
                let modulus = self.name("slot");
                let narrow = self.name("slot_narrow");
                let byte = self.name("slot_byte");
                let exact = self.name("slot_exact");
                let failed = self.name("slot_failed");
                self.body.line(&format!("let {modulus} = {index} % 8_u64;"));
                self.body
                    .line(&format!("let {narrow} = cvt::<u64, u8>({modulus});"));
                self.body.open(&format!("let {byte} = match {narrow} {{"));
                self.body.open(&format!("Ok(value: {exact}) => {{"));
                self.body.line(&format!("give 48_u8 +wrap {exact};"));
                self.body.close();
                self.body.open(&format!("Err(error: {failed}) => {{"));
                self.body.line("give 48_u8;");
                self.body.close();
                self.body.close();
                self.body.line(&format!("set {name}[2_u64] = {byte};"));
            }
            None => {
                self.body
                    .line(&format!("set {name}[2_u64] = {}_u8;", 48 + fixed));
            }
        }
        self.body.line(&format!("set {name}[3_u64] = 46_u8;"));
        self.body.line(&format!("set {name}[4_u64] = 100_u8;"));
        self.body.line(&format!("set {name}[5_u64] = 97_u8;"));
        self.body.line(&format!("set {name}[6_u64] = 116_u8;"));
        name
    }

    fn arithmetic_block(&mut self) {
        let count = self.rng.between(2, 5);
        for _ in 0..count {
            self.arithmetic_statement();
        }
    }

    fn arithmetic_statement(&mut self) {
        self.spend();
        let name = self.name("value");
        let left = self.scalar_atom();
        let choice = self.rng.below(10);
        let text = match choice {
            0 => {
                let right = self.scalar_atom();
                format!("let {name} = {left} +wrap {right};")
            }
            1 => {
                let right = self.scalar_atom();
                format!("let {name} = {left} -wrap {right};")
            }
            2 => {
                let right = self.scalar_atom();
                format!("let {name} = {left} *wrap {right};")
            }
            3 => {
                let divisor = self.rng.between(2, 97);
                format!("let {name} = {left} % {divisor}_u64;")
            }
            4 => {
                let divisor = self.rng.between(2, 97);
                format!("let {name} = {left} / {divisor}_u64;")
            }
            5 => {
                let right = self.scalar_atom();
                format!("let {name} = iand({left}, {right});")
            }
            6 => {
                let right = self.scalar_atom();
                format!("let {name} = ior({left}, {right});")
            }
            7 => {
                let right = self.scalar_atom();
                format!("let {name} = ixor({left}, {right});")
            }
            8 => {
                let right = self.scalar_atom();
                format!("let {name} = imin({left}, {right});")
            }
            _ => {
                let right = self.scalar_atom();
                format!("let {name} = imax({left}, {right});")
            }
        };
        self.body.line(&text);
        self.scalars.push(name.clone());
        if self.rng.chance(70) {
            self.spend();
            self.body.line(&format!("set total = total +wrap {name};"));
        }
    }

    /// A handful of statements for a nested body: arithmetic, an accumulator
    /// update, a guarded subscript, or one more level of branching.
    fn inner_statements(&mut self, index: Option<String>) {
        let count = self.rng.between(1, 4);
        for _ in 0..count {
            if !self.budget() {
                return;
            }
            match self.rng.below(10) {
                0..=4 => self.arithmetic_statement(),
                5..=6 => {
                    self.spend();
                    let atom = self.scalar_atom();
                    self.body.line(&format!("set total = total +wrap {atom};"));
                }
                7..=8 => self.guarded_subscript(index.clone()),
                _ => {
                    if self.depth < 3 {
                        self.branch_block();
                    } else {
                        self.arithmetic_statement();
                    }
                }
            }
        }
    }

    /// A subscript whose bound is established by an explicit `len` comparison,
    /// which is the writer form [OP-4] admits when the index is not the binder
    /// of a `for` over exactly that length.
    fn guarded_subscript(&mut self, index: Option<String>) {
        if self.buffers.is_empty() {
            self.arithmetic_statement();
            return;
        }
        let slot = self.rng.below(self.buffers.len() as u64) as usize;
        let target = self.buffers[slot].name.clone();
        let length = self.buffers[slot].length;
        // A constant position below a length the generator knows needs no
        // guard: [OP-9] publishes the length `buffer_new` established and the
        // literal discharges the [OP-4] obligation on its own.
        if index.is_none() && length > 0 && self.rng.chance(40) {
            let position = self.rng.below(length);
            let byte = self.byte_literal();
            self.spend();
            self.body
                .line(&format!("set {target}[{position}_u64] = {byte};"));
            return;
        }
        let position = match index {
            Some(binder) => binder,
            None => self.scalar_atom(),
        };
        let room = self.name("room");
        let ok = self.name("writable");
        let byte = self.byte_literal();
        self.spend();
        self.body.line(&format!("let {room} = len_of({target});"));
        self.body.line(&format!("let {ok} = {position} < {room};"));
        self.body.open(&format!("if {ok} {{"));
        self.body
            .line(&format!("set {target}[{position}] = {byte};"));
        self.body.close();
    }

    fn branch_block(&mut self) {
        let flag = self.name("flag");
        let condition = self.condition();
        self.spend();
        self.body.line(&format!("let {flag} = {condition};"));
        self.body.open(&format!("if {flag} {{"));
        self.scope(|generator| generator.inner_statements(None));
        if self.rng.chance(60) {
            self.body.indent -= 1;
            self.body.line("} else {");
            self.body.indent += 1;
            self.scope(|generator| generator.inner_statements(None));
        }
        self.body.close();
    }

    fn counted_loop_block(&mut self) {
        let length = *self.rng.pick(&[8_u64, 16, 24, 32, 48, 64]);
        let carry = self.rng.chance(60);
        let target = if carry {
            let fill = self.rng.between(48, 90);
            Some(self.declare_buffer(length, fill))
        } else {
            None
        };
        let label = self.label();
        let binder = self.name("step");
        self.spend();
        self.body
            .open(&format!("for {label} ({binder} in 0_u64..{length}_u64) {{"));
        let inner = binder.clone();
        self.scope(|generator| {
            generator.scalars.push(inner.clone());
            if let Some(store) = &target {
                // The binder's upper endpoint is exactly this buffer's length,
                // so [ENT-3]'s structural fact discharges the subscript with no
                // written guard: the shape P11 names.
                let byte = generator.byte_literal();
                generator.spend();
                generator
                    .body
                    .line(&format!("set {store}[{inner}] = {byte};"));
            }
            generator.inner_statements(Some(inner.clone()));
            if generator.depth < 3 && generator.rng.chance(25) && generator.budget() {
                generator.shapes.insert(Shape::NestedLoop);
                generator.nested_counted_loop();
            }
            generator.spend();
            generator
                .body
                .line(&format!("set total = total +wrap {inner};"));
        });
        self.body.close();
        if let Some(store) = target {
            if self.rng.chance(55) {
                self.fold_buffer(&store, length);
            }
        }
    }

    /// The one loop shape [PAR-2] can permit: exactly one place rooted outside
    /// the loop, written by exactly one `set` under one fixed associative,
    /// commutative operation with an identity, and no other occurrence of that
    /// binding anywhere in the body. Every other statement reads only the
    /// binder and literals, so nothing else leaves the iteration.
    fn accumulator_loop_block(&mut self) {
        let bound = *self.rng.pick(&[8_u64, 16, 32, 64]);
        let label = self.label();
        let binder = self.name("counted");
        let infix = self.rng.chance(50);
        let operation = if infix {
            *self.rng.pick(&["+wrap", "*wrap"])
        } else {
            *self.rng.pick(&["iand", "ior", "ixor", "imin", "imax"])
        };
        self.spend();
        self.body
            .open(&format!("for {label} ({binder} in 0_u64..{bound}_u64) {{"));
        let mut carrier = binder.clone();
        let steps = self.rng.between(1, 3);
        for _ in 0..steps {
            let name = self.name("term");
            let literal = self.rng.between(1, 97);
            let form = match self.rng.below(5) {
                0 => format!("let {name} = {carrier} +wrap {literal}_u64;"),
                1 => format!("let {name} = {carrier} *wrap {literal}_u64;"),
                2 => format!("let {name} = {carrier} % {literal}_u64;"),
                3 => format!("let {name} = ixor({carrier}, {literal}_u64);"),
                _ => format!("let {name} = imax({carrier}, {literal}_u64);"),
            };
            self.spend();
            self.body.line(&form);
            carrier = name;
        }
        if infix {
            self.body
                .line(&format!("set total = total {operation} {carrier};"));
        } else {
            self.body
                .line(&format!("set total = {operation}(total, {carrier});"));
        }
        self.body.close();
    }

    fn nested_counted_loop(&mut self) {
        let bound = *self.rng.pick(&[4_u64, 6, 8]);
        let label = self.label();
        let binder = self.name("inner");
        self.spend();
        self.body
            .open(&format!("for {label} ({binder} in 0_u64..{bound}_u64) {{"));
        let inner = binder.clone();
        self.scope(|generator| {
            generator.scalars.push(inner.clone());
            generator.inner_statements(None);
            generator.spend();
            generator
                .body
                .line(&format!("set total = total +wrap {inner};"));
        });
        self.body.close();
    }

    fn unbounded_loop_block(&mut self) {
        let bound = *self.rng.pick(&[4_u64, 8, 12, 20]);
        let counter = self.name("tick");
        let label = self.label();
        let done = self.name("done");
        self.spend();
        self.body.line(&format!("let {counter} = 0_u64;"));
        self.body.open(&format!("loop {label} {{"));
        let inner = counter.clone();
        let stop = done.clone();
        let exit = label.clone();
        self.scope(|generator| {
            generator.scalars.push(inner.clone());
            generator
                .body
                .line(&format!("let {stop} = {inner} >= {bound}_u64;"));
            generator.body.open(&format!("if {stop} {{"));
            generator.body.line(&format!("break {exit};"));
            generator.body.close();
            generator.inner_statements(None);
            generator.spend();
            generator
                .body
                .line(&format!("set total = total +wrap {inner};"));
            generator
                .body
                .line(&format!("set {inner} = {inner} +wrap 1_u64;"));
        });
        self.body.close();
    }

    fn fold_buffer(&mut self, store: &str, length: u64) {
        self.need_fold = true;
        let digest = self.name("digest");
        self.spend();
        self.body.open("region {");
        self.body.line(&format!(
            "let {digest} = fold_prefix(source: &{store}, produced: {length}_u64, seed: 7_u64);"
        ));
        self.body
            .line(&format!("set total = total +wrap {digest};"));
        self.body.close();
    }

    /// One publication through one `OutputStream`, with both outcomes handled.
    fn write_block(&mut self, to_error: bool) {
        let length = *self.rng.pick(&[4_u64, 8, 16, 24, 32]);
        let fill = self.rng.between(48, 90);
        let store = self.declare_buffer(length, fill);
        let sink = if to_error { "err" } else { "out" };
        let ok = self.name("wrote");
        let failed = self.name("write_failed");
        self.spend();
        self.body.open("region {");
        self.body.open(&format!(
            "match publish_bytes(factory: &uniq files, output: &uniq {sink}, source: &{store}, start: 0_u64, end: {length}_u64) {{"
        ));
        self.body.open(&format!("Ok(value: {ok}) => {{"));
        self.body.line(&format!("set total = total +wrap {ok};"));
        self.body.close();
        self.body.open(&format!("Err(error: {failed}) => {{"));
        self.body.line("set total = total +wrap 3_u64;");
        self.body.close();
        self.body.close();
        self.body.close();
    }

    /// Enough bytes to exceed a host pipe buffer, so a delayed reader on the
    /// other end makes the write genuinely wait.
    fn bulk_write_block(&mut self) {
        let length = *self.rng.pick(&[98304_u64, 131072, 196608]);
        let fill = self.rng.between(65, 90);
        let store = self.declare_buffer(length, fill);
        self.bulk_output = true;
        let ok = self.name("bulk_wrote");
        let failed = self.name("bulk_failed");
        self.spend();
        self.body.open("region {");
        self.body.open(&format!(
            "match publish_bytes(factory: &uniq files, output: &uniq out, source: &{store}, start: 0_u64, end: {length}_u64) {{"
        ));
        self.body.open(&format!("Ok(value: {ok}) => {{"));
        self.body.line(&format!("set total = total +wrap {ok};"));
        self.body.close();
        self.body.open(&format!("Err(error: {failed}) => {{"));
        self.body.line("set total = total +wrap 5_u64;");
        self.body.close();
        self.body.close();
        self.body.close();
    }

    /// Two adjacent publication statements, either to distinct stream wrappers
    /// or to stdout twice. The explicit shared factory state preserves order
    /// even when both wrappers are redirected to one native file. Shared-source
    /// variants additionally borrow the same backing buffer twice.
    fn output_pair_block(&mut self, independent: bool, shared_source: bool) {
        let length = *self.rng.pick(&[8_u64, 16, 32, 48]);
        let left = self.declare_buffer(length, 65);
        let right = if shared_source {
            left.clone()
        } else {
            self.declare_buffer(length, 66)
        };
        let second_sink = if independent { "err" } else { "out" };
        let first = self.name("first");
        let second = self.name("second");
        self.spend();
        self.body.open("region {");
        self.body.open("region {");
        self.body.line(&format!(
            "let {first} = publish_bytes(factory: &uniq files, output: &uniq out, source: &{left}, start: 0_u64, end: {length}_u64);"
        ));
        self.body.line(&format!(
            "let {second} = publish_bytes(factory: &uniq files, output: &uniq {second_sink}, source: &{right}, start: 0_u64, end: {length}_u64);"
        ));
        for binding in [first, second] {
            let ok = self.name("reached");
            let failed = self.name("pair_failed");
            self.body.open(&format!("match {binding} {{"));
            self.body.open(&format!("Ok(value: {ok}) => {{"));
            self.body.line(&format!("set total = total +wrap {ok};"));
            self.body.close();
            self.body.open(&format!("Err(error: {failed}) => {{"));
            self.body.line("set total = total +wrap 11_u64;");
            self.body.close();
            self.body.close();
        }
        self.body.close();
        self.body.close();
    }

    /// Two adjacent pure user calls: the cheapest pair [PAR-1] permits, and the
    /// one whose overlap can only be observed through a wrong result.
    fn pure_call_pair_block(&mut self) {
        self.need_mix = true;
        let first = self.name("mixed");
        let second = self.name("mixed");
        let a = self.scalar_atom();
        let b = self.scalar_atom();
        let c = self.scalar_atom();
        let d = self.scalar_atom();
        self.spend();
        self.body
            .line(&format!("let {first} = mix(a: {a}, b: {b});"));
        self.body
            .line(&format!("let {second} = mix(a: {c}, b: {d});"));
        self.body.line(&format!("set total = total +wrap {first};"));
        self.body
            .line(&format!("set total = total +wrap {second};"));
        self.scalars.push(first);
        self.scalars.push(second);
    }
}

/// Four retained file-loop storage and control-flow variants. The former
/// PAR-3 stage permission is deleted; these remain observable workloads.
#[derive(Clone, Copy)]
enum FileLoop {
    IterationOwn,
    HoistedScratch,
    BreakAfterSubmission,
    SharedScratch,
}

impl Gen {
    /// A loop that opens and reads one fixture file per iteration. Every
    /// storage/control-flow variant publishes a digest of what it read.
    fn file_loop_block(&mut self, variant: FileLoop) {
        self.need_fold = true;
        let rounds = *self.rng.pick(&[4_u64, 6, 8, 12]);
        let window = *self.rng.pick(&[64_u64, 256, 1024, 4096]);

        // The hoisted variant retains its previous bytes across iterations.
        let hoisted_data = match variant {
            FileLoop::HoistedScratch => Some(self.declare_buffer(window, 0)),
            _ => None,
        };
        let hoisted_name = match variant {
            FileLoop::SharedScratch => Some(self.declare_name(None)),
            _ => None,
        };

        let label = self.label();
        let binder = self.name("index");
        self.spend();
        if let Some(data) = &hoisted_data {
            let retained = self.name("scratch_length");
            self.body.line(&format!("for {label} ("));
            self.body.indent += 1;
            self.body.line(&format!("{binder} in 0_u64..{rounds}_u64,"));
            self.body.line(&format!(
                "invariant {retained}: len_of({data}) >= {window}_u64"
            ));
            self.body.indent -= 1;
            self.body.open(") {");
        } else {
            self.body
                .open(&format!("for {label} ({binder} in 0_u64..{rounds}_u64) {{"));
        }
        let binder_name = binder.clone();
        let loop_label = label.clone();
        self.scope(|generator| {
            generator.scalars.push(binder_name.clone());
            let name = match &hoisted_name {
                Some(existing) => {
                    // The shared name buffer is written every iteration, which
                    // is a write to storage the body does not introduce.
                    let byte = generator.byte_literal();
                    generator
                        .body
                        .line(&format!("set {existing}[7_u64] = {byte};"));
                    existing.clone()
                }
                None => generator.declare_name(Some(&binder_name)),
            };
            let data = match &hoisted_data {
                Some(existing) => existing.clone(),
                None => generator.declare_buffer(window, 0),
            };
            generator.body.open("region {");
            generator.body.open("region {");
            generator.body.open(&format!(
                "match open_named(factory: &uniq files, root: &cwd, name: &{name}, start: 0_u64, end: 7_u64) {{"
            ));
            let handle = generator.name("handle");
            generator.body.open(&format!("Ok(value: {handle}) => {{"));
            generator.body.open("region {");
            generator.body.open("region {");
            generator.body.open(&format!(
                "match read_prefix(factory: &uniq files, file: &uniq {handle}, destination: &uniq {data}, start: 0_u64, end: {window}_u64) {{"
            ));
            let produced = generator.name("produced");
            generator
                .body
                .open(&format!("Ok(value: {produced}) => {{"));
            generator
                .body
                .line(&format!("set total = total +wrap {produced};"));
            // Folding the bytes is what makes a shared destination genuinely
            // order-dependent: after a short read the tail is the previous
            // iteration's bytes.
            let digest = generator.name("read_digest");
            generator.body.open("region {");
            generator.body.line(&format!(
                "let {digest} = fold_prefix(source: &{data}, produced: {window}_u64, seed: {produced});"
            ));
            generator
                .body
                .line(&format!("set total = total +wrap {digest};"));
            generator.body.close();
            if matches!(variant, FileLoop::BreakAfterSubmission) {
                // An edge that leaves the loop from the remainder rather than
                // from the prologue.
                let stop = generator.name("enough");
                generator
                    .body
                    .line(&format!("let {stop} = total >= 1000000000_u64;"));
                generator.body.open(&format!("if {stop} {{"));
                generator.body.line(&format!("close_read(factory: &uniq files, file: move {handle});"));
                generator.body.line(&format!("break {loop_label};"));
                generator.body.close();
            }
            generator.body.close();
            let read_stop = generator.name("read_stop");
            generator.body.open(&format!("Err(error: {read_stop}) => {{"));
            generator.body.open(&format!("match move {read_stop} {{"));
            generator.body.open("ReadEnd() => {");
            generator.body.line("set total = total +wrap 13_u64;");
            generator.body.close();
            let problem = generator.name("read_problem");
            generator
                .body
                .open(&format!("ReadFailed(error: {problem}) => {{"));
            generator.body.line("set total = total +wrap 17_u64;");
            generator.body.close();
            generator.body.close();
            generator.body.close();
            generator.body.close();
            generator.body.close();
            generator.body.close();
            generator.body.line(&format!("close_read(factory: &uniq files, file: move {handle});"));
            generator.body.close();
            let denied = generator.name("open_problem");
            generator.body.open(&format!("Err(error: {denied}) => {{"));
            generator.body.line("set total = total +wrap 19_u64;");
            generator.body.close();
            generator.body.close();
            generator.body.close();
            generator.body.close();
        });
        self.body.close();
    }

    /// Opens one file, reads it, and publishes what it read. The write reads
    /// the buffer the read wrote, so no permission may overlap the two, and the
    /// published bytes are the file's.
    fn read_then_write_block(&mut self) {
        let window = *self.rng.pick(&[32_u64, 64, 128, 256]);
        let name = self.declare_name(None);
        let data = self.declare_buffer(window, 46);
        let got = self.name("got");
        self.body.line(&format!("let {got} = 0_u64;"));
        self.scalars.push(got.clone());
        self.spend();
        self.body.open("region {");
        self.body.open("region {");
        self.body.open(&format!(
            "match open_named(factory: &uniq files, root: &cwd, name: &{name}, start: 0_u64, end: 7_u64) {{"
        ));
        let handle = self.name("handle");
        self.body
            .open(&format!("Ok(value: {handle}) => {{"));
        self.body.open("region {");
        self.body.open("region {");
        self.body.open(&format!(
            "match read_prefix(factory: &uniq files, file: &uniq {handle}, destination: &uniq {data}, start: 0_u64, end: {window}_u64) {{"
        ));
        let produced = self.name("produced");
        self.body.open(&format!("Ok(value: {produced}) => {{"));
        self.body.line(&format!("set {got} = {produced};"));
        self.body.close();
        let read_stop = self.name("read_stop");
        self.body.open(&format!("Err(error: {read_stop}) => {{"));
        self.body.open(&format!("match move {read_stop} {{"));
        self.body.open("ReadEnd() => {");
        self.body.close();
        let problem = self.name("read_problem");
        self.body
            .open(&format!("ReadFailed(error: {problem}) => {{"));
        self.body.close();
        self.body.close();
        self.body.close();
        self.body.close();
        self.body.close();
        self.body.close();
        self.body.line(&format!(
            "close_read(factory: &uniq files, file: move {handle});"
        ));
        self.body.close();
        let denied = self.name("open_problem");
        self.body
            .open(&format!("Err(error: {denied}) => {{"));
        self.body.close();
        self.body.close();
        self.body.close();
        self.body.close();

        // Preserve the original conditional publication workload. The ordinary
        // helper's source requirement owns the window bound.
        let room = self.name("room");
        let publishable = self.name("publishable");
        let ok = self.name("relayed");
        let failed = self.name("relay_failed");
        self.body.line(&format!("let {room} = len_of({data});"));
        self.body
            .line(&format!("let {publishable} = {got} <= {room};"));
        self.body.open(&format!("if {publishable} {{"));
        self.body.open("region {");
        self.body.open(&format!(
            "match publish_bytes(factory: &uniq files, output: &uniq out, source: &{data}, start: 0_u64, end: {got}) {{"
        ));
        self.body.open(&format!("Ok(value: {ok}) => {{"));
        self.body.line(&format!("set total = total +wrap {ok};"));
        self.body.close();
        self.body.open(&format!("Err(error: {failed}) => {{"));
        self.body.line("set total = total +wrap 23_u64;");
        self.body.close();
        self.body.close();
        self.body.close();
        self.body.close();
    }

    /// Directory enumeration. Only the reported counts reach the digest,
    /// because the byte order of one batch is the host's and folding it would
    /// make the program its own unstable oracle.
    fn directory_block(&mut self) {
        let entries = self.declare_buffer(4096, 0);
        let ended = self.name("ended");
        self.body.line(&format!("let {ended} = 0_u8;"));
        self.spend();
        self.body.open("region {");
        self.body
            .open("match open_directory_source(factory: &uniq files, directory: &cwd) {");
        let source = self.name("listing");
        self.body
            .open(&format!("Ok(value: {source}) => {{"));
        let rounds = self.name("rounds");
        let label = self.label();
        let stop = self.name("stop");
        let live = self.name("live");
        self.body.line(&format!("let {rounds} = 0_u64;"));
        let retained = self.name("entries_length");
        self.body.line(&format!("loop {label} ("));
        self.body.indent += 1;
        self.body.line(&format!(
            "invariant {retained}: len_of({entries}) >= 4096_u64"
        ));
        self.body.indent -= 1;
        self.body.open(") {");
        self.body.line(&format!("let {stop} = {rounds} >= 8_u64;"));
        self.body.open(&format!("if {stop} {{"));
        self.body.line(&format!("break {label};"));
        self.body.close();
        self.body.open("region {");
        let endpoint = self.name("endpoint");
        let reported = self.name("reported");
        let result = self.name("batch_result");
        let done = self.name("batch_done");
        self.body.line(&format!("let ({result}, {endpoint}, {reported}) = list_batch(source: &uniq {source}, destination: &uniq {entries}, start: 0_u64, end: 4096_u64);"));
        self.body.open(&format!("match move {result} {{"));
        self.body.open(&format!("Ok(value: {done}) => {{"));
        self.body
            .line(&format!("set total = total +wrap {reported};"));
        self.body.close();
        let stop = self.name("list_stop");
        self.body.open(&format!("Err(error: {stop}) => {{"));
        self.body.open(&format!("match move {stop} {{"));
        self.body.open("ListEnd() => {");
        self.body.line(&format!("set {ended} = 1_u8;"));
        self.body.close();
        let problem = self.name("list_problem");
        self.body
            .open(&format!("ListFailed(error: {problem}) => {{"));
        self.body.line(&format!("set {ended} = 2_u8;"));
        self.body.close();
        self.body.close();
        self.body.close();
        self.body.close();
        self.body.close();
        self.body.line(&format!("let {live} = {ended} == 0_u8;"));
        self.body.open(&format!("if {live} {{"));
        self.body.indent -= 1;
        self.body.line("} else {");
        self.body.indent += 1;
        self.body.line(&format!("break {label};"));
        self.body.close();
        self.body
            .line(&format!("set {rounds} = {rounds} +wrap 1_u64;"));
        self.body.close();
        self.body.line(&format!(
            "close_directory_source(factory: &uniq files, source: move {source});"
        ));
        self.body.close();
        let denied = self.name("source_problem");
        self.body
            .open(&format!("Err(error: {denied}) => {{"));
        self.body.line("set total = total +wrap 29_u64;");
        self.body.close();
        self.body.close();
        self.body.close();
    }

    /// The former claim shape now states an erased invariant over a local
    /// unsigned remainder. The indexed mutation and digest stay unchanged.
    fn invariant_block(&mut self) {
        let length = *self.rng.pick(&[8_u64, 16, 32, 64]);
        let store = self.declare_buffer(length, 65);
        let seed = self.name("seed");
        let index = self.name("offset");
        let guard = self.name("guard");
        let literal = self.rng.between(1, 4096);
        self.spend();
        self.body.line(&format!("let {seed} = {literal}_u64;"));
        self.body
            .line(&format!("let {index} = {seed} % {length}_u64;"));
        self.body
            .line(&format!("invariant {guard}: {index} < {length}_u64;"));
        let byte = self.byte_literal();
        self.body.line(&format!("set {store}[{index}] = {byte};"));
        self.body.line(&format!("set total = total +wrap {index};"));
        self.scalars.push(index);
    }

    /// A direct view over a live buffer, moved into a helper that reads through
    /// it (P10). The slice descriptor carries a finite static origin set, so the
    /// footprint the permission judgment forms for the call is the origin's, not
    /// the descriptor's -- which is exactly the thing worth putting under an
    /// overlap oracle.
    fn slice_view_block(&mut self) {
        self.need_view = true;
        let (store, length) = match self.buffers.last() {
            Some(buffer) => (buffer.name.clone(), buffer.length),
            None => {
                let length = *self.rng.pick(&[8_u64, 16, 32, 64]);
                let fill = self.rng.between(48, 90);
                (self.declare_buffer(length, fill), length)
            }
        };
        let view = self.name("view");
        let room = self.name("view_room");
        let digest = self.name("view_digest");
        let seed = self.rng.between(1, 97);
        self.spend();
        self.body.open("region {");
        self.body.line(&format!("let {view} = slice_of(&{store});"));
        self.body.line(&format!("let {room} = len_of({view});"));
        self.body.line(&format!(
            "let {digest} = fold_view(view: {view}, produced: {length}_u64, seed: {seed}_u64);"
        ));
        self.body
            .line(&format!("set total = total +wrap {digest};"));
        self.body.line(&format!("set total = total +wrap {room};"));
        self.body.close();
    }

    /// A conditional value: the `let`-initializer `match` of [GRAM-7], whose
    /// arms deliver through [GIVE-1].
    fn give_match_block(&mut self) {
        let source = self.scalar_atom();
        let narrow = self.name("narrowed");
        let picked = self.name("picked");
        let exact = self.name("exact");
        let widened = self.name("widened");
        let failed = self.name("narrow_failed");
        let fallback = self.rng.between(100, 4000);
        self.spend();
        self.body
            .line(&format!("let {narrow} = cvt::<u64, u8>({source});"));
        self.body.open(&format!("let {picked} = match {narrow} {{"));
        self.body.open(&format!("Ok(value: {exact}) => {{"));
        self.body
            .line(&format!("let {widened} = cvt::<u8, u64>({exact});"));
        self.body.line(&format!("give {widened};"));
        self.body.close();
        self.body.open(&format!("Err(error: {failed}) => {{"));
        self.body.line(&format!("give {fallback}_u64;"));
        self.body.close();
        self.body.close();
        self.body
            .line(&format!("set total = total +wrap {picked};"));
        self.scalars.push(picked);
    }

    /// A publication whose failure arm leaves the program through a typed exit
    /// status rather than through the digest, so an execution that diverges on
    /// the failure edge diverges on the exit status too.
    fn typed_exit_block(&mut self) {
        let length = *self.rng.pick(&[4_u64, 8, 16]);
        let fill = self.rng.between(48, 90);
        let store = self.declare_buffer(length, fill);
        let ok = self.name("exit_wrote");
        let failed = self.name("exit_failed");
        let code = self.rng.between(20, 90);
        self.spend();
        self.body.open("region {");
        self.body.open(&format!(
            "match publish_bytes(factory: &uniq files, output: &uniq out, source: &{store}, start: 0_u64, end: {length}_u64) {{"
        ));
        self.body.open(&format!("Ok(value: {ok}) => {{"));
        self.body.line(&format!("set total = total +wrap {ok};"));
        self.body.close();
        self.body.open(&format!("Err(error: {failed}) => {{"));
        self.close_cwd();
        self.body
            .line(&format!("return exit_status(code: {code}_u8);"));
        self.body.close();
        self.body.close();
        self.body.close();
    }

    /// The invocation snapshot: a count, and one argument's byte length.
    ///
    /// Never position zero. The oracle runs the same program from three
    /// different files -- one per lowering -- so argument zero is the one
    /// invocation datum the harness cannot make identical across the runs it
    /// compares. A program that reads it publishes the harness's file name and
    /// reports a difference that is not the compiler's. Positions one and two
    /// are the two literal arguments the oracle passes, identical everywhere.
    fn argument_block(&mut self) {
        let count = self.name("argument_count");
        self.spend();
        self.body.open("region {");
        self.body
            .line(&format!("let {count} = args_count(args: &args);"));
        self.body.line(&format!("set total = total +wrap {count};"));
        if self.rng.chance(60) {
            let value = self.name("argument");
            let failed = self.name("argument_missing");
            let length = self.name("argument_length");
            let position = self.rng.between(1, 2);
            self.body.open(&format!(
                "match arg_get(args: &args, position: {position}_u64) {{"
            ));
            self.body.open(&format!("Ok(value: {value}) => {{"));
            self.body.open("region {");
            self.body
                .line(&format!("let {length} = host_bytes_len(value: &{value});"));
            self.body
                .line(&format!("set total = total +wrap {length};"));
            self.body.close();
            self.body.close();
            self.body.open(&format!("Err(error: {failed}) => {{"));
            self.body.line("set total = total +wrap 31_u64;");
            self.body.close();
            self.body.close();
        }
        self.body.close();
    }
}

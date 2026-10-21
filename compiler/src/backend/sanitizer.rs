//! The run-time checkers a native build can instrument its program with
//! (compiler/sanitized-builds).
//!
//! An accepted program has no memory error, undefined behavior or data race,
//! so any report a checker makes is a soundness defect of the checker, the
//! compiler or the runtime. A sanitized build is evidence about the compiler
//! and its runtime, never a different language: it changes no acceptance and
//! no lowering choice, only the instrumentation the host compiler adds.

use super::LlvmModule;

/// A host sanitizer configuration for one native build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sanitizer {
    /// AddressSanitizer over the emitted module and the C runtime, with
    /// UndefinedBehaviorSanitizer over the C runtime, the only code clang
    /// compiles from C; `-fno-sanitize-recover=all` stops at the first
    /// undefined operation instead of continuing past it.
    Address,
    /// ThreadSanitizer over the emitted module and the C runtime.
    Thread,
}

impl Sanitizer {
    /// The configuration `--sanitize` names, or `None` for any other word.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "address" => Some(Self::Address),
            "thread" => Some(Self::Thread),
            _ => None,
        }
    }

    /// The word `--sanitize` takes for this configuration.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Address => "address",
            Self::Thread => "thread",
        }
    }

    /// Arguments every compile and the link of a sanitized build add, so the
    /// runtime units, the emitted module and the link agree.
    ///
    /// Use-after-return detection stays off: it moves address-taken locals to
    /// a heap-allocated fake stack, so a deep recursion would no longer reach
    /// the guard band the floor runtime reports stack exhaustion from.
    #[must_use]
    pub const fn clang_arguments(self) -> &'static [&'static str] {
        match self {
            Self::Address => &[
                "-fsanitize=address,undefined",
                "-fno-sanitize-recover=all",
                "-fsanitize-address-use-after-return=never",
                "-fno-omit-frame-pointer",
            ],
            Self::Thread => &["-fsanitize=thread", "-fno-omit-frame-pointer"],
        }
    }

    /// The LLVM function attribute that admits a definition to the
    /// sanitizer's instrumentation pass: clang adds it to the C it compiles,
    /// but never to definitions it reads as LLVM.
    #[must_use]
    pub const fn function_attribute(self) -> &'static str {
        match self {
            Self::Address => "sanitize_address",
            Self::Thread => "sanitize_thread",
        }
    }
}

impl LlvmModule {
    /// This module with every generated definition admitted to `sanitizer`'s
    /// instrumentation. Every definition the emitter writes carries attribute
    /// group 0, so the attribute joins that group; a sanitized build applies
    /// it to the complete module it links, after every composition.
    #[must_use]
    pub fn sanitized(mut self, sanitizer: Sanitizer) -> Self {
        self.model
            .add_group_attribute(0, sanitizer.function_attribute());
        self.rerender();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::Sanitizer;
    use crate::{CompilerLimits, SourceInput, compile};

    #[test]
    fn every_emitted_definition_is_admitted_to_the_selected_sanitizer() {
        let source = b"fn main() -> result: u8 pure {\n  return 7_u8;\n}\n";
        let module = compile(
            &[SourceInput::new("entry.wf", source)],
            CompilerLimits::default(),
        )
        .expect("a u8 entry compiles");
        assert!(!module.contains("sanitize_"), "{}", module.as_str());
        for sanitizer in [Sanitizer::Address, Sanitizer::Thread] {
            let text = module.clone().sanitized(sanitizer).into_string();
            let groups = text
                .lines()
                .filter(|line| line.starts_with("attributes #"))
                .collect::<Vec<_>>();
            assert_eq!(groups.len(), 1, "{text}");
            assert!(
                groups[0].starts_with("attributes #0 = {")
                    && groups[0].contains(sanitizer.function_attribute()),
                "{}",
                groups[0]
            );
            let definitions = text.lines().filter(|line| line.starts_with("define "));
            for definition in definitions {
                assert!(definition.contains(" #0"), "{definition}");
            }
            assert_eq!(Sanitizer::parse(sanitizer.name()), Some(sanitizer));
        }
        assert_eq!(Sanitizer::parse("memory"), None);
    }
}

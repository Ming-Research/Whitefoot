//! Source presentation with positions that can be rebound after a cache hit.

use core::fmt;

use super::{ModuleId, ModuleRecord, SourceInput};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DescriptionPosition {
    /// Qualified module name; `None` identifies a synthetic source bundle.
    pub(crate) module: Option<String>,
    pub(crate) logical_path: String,
    pub(crate) display_path: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DescriptionPart {
    Literal(String),
    Position(DescriptionPosition),
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum DescriptionIdentityPart {
    Literal(String),
    Position(Option<String>, String, usize, usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceReading {
    pub(crate) module: Option<String>,
    pub(crate) logical_path: String,
    pub(crate) display_path: String,
}

impl SourceReading {
    pub(crate) fn matches(
        &self,
        inputs: &[SourceInput<'_>],
        modules: Option<&[ModuleRecord]>,
    ) -> bool {
        source_input(inputs, modules, self.module.as_deref(), &self.logical_path)
            .is_some_and(|input| input.display_path() == self.display_path)
    }
}

fn source_input<'a, 'input>(
    inputs: &'a [SourceInput<'input>],
    modules: Option<&[ModuleRecord]>,
    module: Option<&str>,
    logical_path: &str,
) -> Option<&'a SourceInput<'input>> {
    inputs.iter().find(|input| {
        input.logical_path() == logical_path
            && match modules {
                Some(modules) => modules.get(input.module().index()).is_some_and(|record| {
                    module == Some(record.qualified_name().as_str())
                }),
                None => module.is_none() && input.module() == ModuleId::BUNDLE_ROOT,
            }
    })
}

/// A presentation template containing literal text and source positions.
///
/// The source identity and coordinates remain fixed when an invocation changes
/// its host spelling. Only the display binding is replaced, never literal text.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceDescription {
    pub(crate) parts: Vec<DescriptionPart>,
}

impl SourceDescription {
    pub(crate) fn literal(text: impl Into<String>) -> Self {
        Self {
            parts: vec![DescriptionPart::Literal(text.into())],
        }
    }

    pub(crate) fn position(position: DescriptionPosition) -> Self {
        Self {
            parts: vec![DescriptionPart::Position(position)],
        }
    }

    pub(crate) fn push_text(&mut self, text: impl Into<String>) {
        self.parts.push(DescriptionPart::Literal(text.into()));
    }

    pub(crate) fn append(&mut self, other: &Self) {
        self.parts.extend_from_slice(&other.parts);
    }

    /// Presentation ordering and deduplication use logical source identity,
    /// so rebinding display paths cannot reorder a cached description.
    pub(crate) fn identity(&self) -> Vec<DescriptionIdentityPart> {
        self.parts
            .iter()
            .map(|part| match part {
                DescriptionPart::Literal(text) => DescriptionIdentityPart::Literal(text.clone()),
                DescriptionPart::Position(position) => DescriptionIdentityPart::Position(
                    position.module.clone(),
                    position.logical_path.clone(),
                    position.line,
                    position.column,
                ),
            })
            .collect()
    }

    /// A missing source binding makes a cached emission unusable. In-memory
    /// paths are valid; no host filesystem resolution participates here.
    pub(crate) fn rebind(
        &mut self,
        inputs: &[SourceInput<'_>],
        modules: Option<&[ModuleRecord]>,
    ) -> Option<()> {
        for part in &mut self.parts {
            if let DescriptionPart::Position(position) = part {
                let input = source_input(
                    inputs,
                    modules,
                    position.module.as_deref(),
                    &position.logical_path,
                )?;
                position.display_path = input.display_path().to_owned();
            }
        }
        Some(())
    }
}

impl fmt::Display for SourceDescription {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        for part in &self.parts {
            match part {
                DescriptionPart::Literal(text) => output.write_str(text)?,
                DescriptionPart::Position(position) => write!(
                    output,
                    "{}:{}:{}",
                    position.display_path, position.line, position.column
                )?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_identity_ignores_display_path_and_orders_coordinates() {
        let position = |module, logical: &str, display: &str, line| {
            SourceDescription::position(DescriptionPosition {
                module,
                logical_path: logical.to_owned(),
                display_path: display.to_owned(),
                line,
                column: 3,
            })
        };
        let site = |display: &str, line| position(None, "main.wf", display, line);
        assert_eq!(
            site("old/main.wf", 2).identity(),
            site("new/main.wf", 2).identity()
        );
        assert!(site("z/main.wf", 2).identity() < site("a/main.wf", 10).identity());
        assert_ne!(
            site("old/main.wf", 2).to_string(),
            site("new/main.wf", 2).to_string()
        );
        assert_ne!(
            site("old/main.wf", 2).identity(),
            position(None, "other.wf", "old/main.wf", 2).identity()
        );
        assert_ne!(
            site("old/main.wf", 2).identity(),
            position(
                Some("pkg::other".to_owned()),
                "main.wf",
                "old/main.wf",
                2,
            )
            .identity()
        );

        let mut description = SourceDescription::literal("old/main.wf mentioned literally; site ");
        description.append(&site("old/main.wf", 2));
        let identity = description.identity();
        let input = SourceInput::from_host_path("main.wf", "new/main.wf", b"");
        assert_eq!(description.rebind(&[input], None), Some(()));
        assert_eq!(description.identity(), identity);
        assert_eq!(
            description.to_string(),
            "old/main.wf mentioned literally; site new/main.wf:2:3"
        );
        let missing = SourceInput::from_host_path("other.wf", "new/main.wf", b"");
        assert_eq!(description.rebind(&[missing], None), None);
    }

    #[test]
    fn description_rebinding_uses_module_names_after_graph_reordering() {
        let modules = [
            ModuleRecord::new(vec!["other".to_owned()], Vec::new()),
            ModuleRecord::new(vec!["worker".to_owned()], Vec::new()),
        ];
        let mut description = SourceDescription::position(DescriptionPosition {
            module: Some(modules[1].qualified_name()),
            logical_path: "main.wf".to_owned(),
            display_path: "old/worker.wf".to_owned(),
            line: 2,
            column: 3,
        });
        let inputs = [
            SourceInput::from_host_path("main.wf", "new/other.wf", b""),
            SourceInput::from_host_path("main.wf", "new/worker.wf", b"").in_module(
                ModuleId::from_index(1).expect("second graph row"),
                crate::SourceRole::Implementation,
            ),
        ];
        assert_eq!(description.rebind(&inputs, Some(&modules)), Some(()));
        assert_eq!(description.to_string(), "new/worker.wf:2:3");

        let modules = [modules[1].clone(), modules[0].clone()];
        let inputs = [
            SourceInput::from_host_path("main.wf", "moved/worker.wf", b""),
            SourceInput::from_host_path("main.wf", "moved/other.wf", b"").in_module(
                ModuleId::from_index(1).expect("second graph row"),
                crate::SourceRole::Implementation,
            ),
        ];
        assert_eq!(description.rebind(&inputs, Some(&modules)), Some(()));
        assert_eq!(description.to_string(), "moved/worker.wf:2:3");
    }
}

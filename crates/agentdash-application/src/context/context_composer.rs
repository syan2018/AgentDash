use std::collections::HashMap;

use agentdash_spi::{ContextFragment, MergeStrategy};

#[derive(Default)]
pub struct ContextComposer {
    fragments: Vec<ContextFragment>,
}

impl ContextComposer {
    pub fn push(
        &mut self,
        slot: &'static str,
        label: &'static str,
        order: i32,
        strategy: MergeStrategy,
        content: impl Into<String>,
    ) {
        let content = content.into();
        if content.trim().is_empty() {
            return;
        }
        self.fragments.push(ContextFragment {
            slot: slot.to_string(),
            label: label.to_string(),
            order,
            strategy,
            scope: ContextFragment::default_scope(),
            source: "legacy:context_composer".to_string(),
            content,
        });
    }

    pub fn push_fragment(&mut self, fragment: ContextFragment) {
        if !fragment.content.trim().is_empty() {
            self.fragments.push(fragment);
        }
    }

    pub fn compose(mut self) -> (String, Vec<String>) {
        self.fragments.sort_by_key(|item| item.order);

        let mut slot_order: Vec<String> = Vec::new();
        let mut slot_chunks: HashMap<String, Vec<String>> = HashMap::new();
        let mut source_summary: Vec<String> = Vec::new();

        for fragment in self.fragments {
            if !slot_chunks.contains_key(&fragment.slot) {
                slot_order.push(fragment.slot.clone());
            }
            source_summary.push(format!("{}({})", fragment.label, fragment.slot));

            match fragment.strategy {
                MergeStrategy::Append => {
                    slot_chunks
                        .entry(fragment.slot)
                        .or_default()
                        .push(fragment.content);
                }
                MergeStrategy::Override => {
                    slot_chunks.insert(fragment.slot, vec![fragment.content]);
                }
            }
        }

        let mut sections = Vec::new();
        for slot in slot_order {
            if let Some(chunks) = slot_chunks.remove(&slot) {
                let merged = chunks
                    .into_iter()
                    .filter(|chunk| !chunk.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                if !merged.trim().is_empty() {
                    sections.push(merged);
                }
            }
        }

        (sections.join("\n\n"), source_summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_only_affects_same_slot() {
        let mut composer = ContextComposer::default();
        composer.push(
            "requirements",
            "manual_text",
            10,
            MergeStrategy::Append,
            "req-a",
        );
        composer.push(
            "instruction",
            "instruction_base",
            20,
            MergeStrategy::Override,
            "run-a",
        );

        let (prompt, summary) = composer.compose();
        assert!(prompt.contains("req-a"));
        assert!(prompt.contains("run-a"));
        assert_eq!(summary.len(), 2);
    }
}

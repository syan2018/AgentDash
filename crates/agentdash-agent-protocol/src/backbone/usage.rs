use crate::codex_app_server_protocol as codex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTokenUsageUpdatedNotification {
    pub thread_id: String,
    pub turn_id: String,
    pub token_usage: ThreadTokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTokenUsage {
    pub total: TokenUsageBreakdown,
    pub last: TokenUsageBreakdown,
    #[ts(type = "number | null")]
    pub model_context_window: Option<i64>,
    pub context: NormalizedContextUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageBreakdown {
    #[ts(type = "number")]
    pub total_tokens: i64,
    #[ts(type = "number")]
    pub input_tokens: i64,
    #[ts(type = "number")]
    pub cached_input_tokens: i64,
    #[ts(type = "number")]
    pub output_tokens: i64,
    #[ts(type = "number")]
    pub reasoning_output_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ContextUsageSource {
    Provider,
    ProviderPlusEstimate,
    LocalEstimate,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedContextUsage {
    #[ts(type = "number")]
    pub provider_context_tokens: i64,
    #[ts(type = "number")]
    pub pending_estimate_tokens: i64,
    #[ts(type = "number")]
    pub current_context_tokens: i64,
    #[ts(type = "number")]
    pub cumulative_total_tokens: i64,
    #[ts(type = "number | null")]
    pub model_context_window: Option<i64>,
    #[ts(type = "number | null")]
    pub effective_context_window: Option<i64>,
    #[ts(type = "number")]
    pub reserve_tokens: i64,
    pub source: ContextUsageSource,
}

impl From<codex::ThreadTokenUsageUpdatedNotification> for ThreadTokenUsageUpdatedNotification {
    fn from(value: codex::ThreadTokenUsageUpdatedNotification) -> Self {
        Self {
            thread_id: value.thread_id,
            turn_id: value.turn_id,
            token_usage: value.token_usage.into(),
        }
    }
}

impl From<codex::ThreadTokenUsage> for ThreadTokenUsage {
    fn from(value: codex::ThreadTokenUsage) -> Self {
        let total: TokenUsageBreakdown = value.total.into();
        let last: TokenUsageBreakdown = value.last.into();
        let provider_context_tokens = last.total_tokens.max(0);
        let cumulative_total_tokens = total.total_tokens.max(0);
        let effective_context_window = value.model_context_window.map(|window| window.max(0));
        let context = NormalizedContextUsage {
            provider_context_tokens,
            pending_estimate_tokens: 0,
            current_context_tokens: provider_context_tokens,
            cumulative_total_tokens,
            model_context_window: value.model_context_window.map(|window| window.max(0)),
            effective_context_window,
            reserve_tokens: 0,
            source: ContextUsageSource::Provider,
        };

        Self {
            total,
            last,
            model_context_window: value.model_context_window,
            context,
        }
    }
}

impl From<codex::TokenUsageBreakdown> for TokenUsageBreakdown {
    fn from(value: codex::TokenUsageBreakdown) -> Self {
        Self {
            total_tokens: value.total_tokens,
            input_tokens: value.input_tokens,
            cached_input_tokens: value.cached_input_tokens,
            output_tokens: value.output_tokens,
            reasoning_output_tokens: value.reasoning_output_tokens,
        }
    }
}

impl From<TokenUsageBreakdown> for codex::TokenUsageBreakdown {
    fn from(value: TokenUsageBreakdown) -> Self {
        Self {
            total_tokens: value.total_tokens,
            input_tokens: value.input_tokens,
            cached_input_tokens: value.cached_input_tokens,
            output_tokens: value.output_tokens,
            reasoning_output_tokens: value.reasoning_output_tokens,
        }
    }
}

impl ThreadTokenUsage {
    pub fn from_current_context(used_tokens: u64, context_window: u64) -> Self {
        let used = i64::try_from(used_tokens).unwrap_or(i64::MAX);
        let window = i64::try_from(context_window).unwrap_or(i64::MAX);
        let total = TokenUsageBreakdown {
            total_tokens: used,
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
        };
        let last = total.clone();
        Self {
            total,
            last,
            model_context_window: Some(window),
            context: NormalizedContextUsage {
                provider_context_tokens: 0,
                pending_estimate_tokens: used,
                current_context_tokens: used,
                cumulative_total_tokens: used,
                model_context_window: Some(window),
                effective_context_window: Some(window),
                reserve_tokens: 0,
                source: ContextUsageSource::LocalEstimate,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codex_breakdown(total_tokens: i64) -> codex::TokenUsageBreakdown {
        codex::TokenUsageBreakdown {
            total_tokens,
            input_tokens: total_tokens.saturating_sub(100),
            cached_input_tokens: 40,
            output_tokens: 60,
            reasoning_output_tokens: 40,
        }
    }

    #[test]
    fn codex_last_drives_current_context_and_total_drives_cumulative_usage() {
        let usage = ThreadTokenUsage::from(codex::ThreadTokenUsage {
            last: codex_breakdown(12_000),
            total: codex_breakdown(120_000),
            model_context_window: Some(200_000),
        });

        assert_eq!(usage.context.provider_context_tokens, 12_000);
        assert_eq!(usage.context.current_context_tokens, 12_000);
        assert_eq!(usage.context.cumulative_total_tokens, 120_000);
        assert_eq!(usage.context.effective_context_window, Some(200_000));
        assert!(matches!(usage.context.source, ContextUsageSource::Provider));
    }
}

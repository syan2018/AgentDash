//! Pi Agent 内置系统提示常量。
//!
//! 此模块定义 Layer 0（系统全局）的默认 system prompt，
//! 作为所有 Pi Agent session 的身份基底。
//! 可通过 settings `agent.pi.base_system_prompt` 整体覆盖。

pub const DEFAULT_SYSTEM_PROMPT: &str = include_str!("prompts/default_system_prompt.md");

use codex_app_server_protocol as codex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{ContentBlock, EmbeddedResourceResource};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserInputSubmissionKind {
    Prompt,
    Steer,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UserInputSubmittedNotification {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub submission_kind: UserInputSubmissionKind,
    pub content: Vec<codex::UserInput>,
}

impl UserInputSubmittedNotification {
    pub fn new(
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
        item_id: impl Into<String>,
        submission_kind: UserInputSubmissionKind,
        content: Vec<codex::UserInput>,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            item_id: item_id.into(),
            submission_kind,
            content,
        }
    }
}

pub fn user_input_text(input: &codex::UserInput) -> Option<&str> {
    match input {
        codex::UserInput::Text { text, .. } => Some(text.as_str()),
        _ => None,
    }
}

#[derive(Debug, Error)]
pub enum UserInputConversionError {
    #[error("用户输入中没有可投递到 Codex UserInput 的内容")]
    EmptyCodexInput,
    #[error("Codex UserInput 中没有可投递文本")]
    EmptyTextInput,
}

pub fn content_blocks_to_codex_user_input(
    blocks: &[ContentBlock],
) -> Result<Vec<codex::UserInput>, UserInputConversionError> {
    let input = blocks
        .iter()
        .filter_map(content_block_to_codex_user_input)
        .collect::<Vec<_>>();
    if input.is_empty() {
        return Err(UserInputConversionError::EmptyCodexInput);
    }
    Ok(input)
}

pub fn content_block_to_codex_user_input(block: &ContentBlock) -> Option<codex::UserInput> {
    match block {
        ContentBlock::Text(text) => text_to_user_input(&text.text),
        ContentBlock::Image(image) => {
            let url = image
                .uri
                .clone()
                .unwrap_or_else(|| format!("data:{};base64,{}", image.mime_type, image.data));
            Some(codex::UserInput::Image { detail: None, url })
        }
        ContentBlock::Resource(resource) => match &resource.resource {
            EmbeddedResourceResource::TextResourceContents(text_res) => {
                text_to_user_input(&format!(
                    "\n<file path=\"{}\">\n{}\n</file>",
                    text_res.uri, text_res.text
                ))
            }
            EmbeddedResourceResource::BlobResourceContents(blob_res) => {
                text_to_user_input(&format!(
                    "[引用二进制资源: {}; mimeType={}]",
                    blob_res.uri,
                    blob_res.mime_type.as_deref().unwrap_or("unknown")
                ))
            }
            _ => text_to_user_input("[引用资源: 未知类型]"),
        },
        ContentBlock::ResourceLink(link) => {
            text_to_user_input(&format!("[引用文件: {} ({})]", link.name, link.uri))
        }
        ContentBlock::Audio(audio) => text_to_user_input(&format!(
            "[引用音频: mimeType={}, base64Bytes={}]",
            audio.mime_type,
            audio.data.len()
        )),
        _ => None,
    }
}

pub fn codex_user_input_to_text(
    input: &[codex::UserInput],
) -> Result<String, UserInputConversionError> {
    let text = input
        .iter()
        .filter_map(|item| match item {
            codex::UserInput::Text { text, .. } => Some(text.as_str().to_string()),
            codex::UserInput::Image { url, .. } => Some(url.clone()),
            codex::UserInput::LocalImage { path, .. } => path.to_str().map(ToString::to_string),
            codex::UserInput::Skill { name, .. } => Some(name.clone()),
            codex::UserInput::Mention { name, .. } => Some(name.clone()),
        })
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        return Err(UserInputConversionError::EmptyTextInput);
    }
    Ok(text)
}

fn text_to_user_input(text: &str) -> Option<codex::UserInput> {
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(codex::UserInput::Text {
            text: text.to_string(),
            text_elements: Vec::new(),
        })
    }
}

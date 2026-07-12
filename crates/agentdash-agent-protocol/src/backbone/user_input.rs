use crate::codex_app_server_protocol as codex;
use agentdash_agent_types::ContentPart;
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{ContentBlock, EmbeddedResourceResource};

/// 全项目 canonical 的"用户输入单元"。
///
/// 当前是 Codex app-server v2 `UserInput` 的别名（接缝层）：调用方一律用此名，
/// 不直接 `use codex_app_server_protocol::UserInput`，为后续替换为自定义扩展类型留接缝。
/// 若将来需要扩展（如 ACP Resource/ResourceLink 文件引用），把此别名升级为本地 `enum`
/// 并在边界补 `From/Into codex::UserInput`，调用方无需改动。
pub type UserInputBlock = codex::UserInput;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserInputSubmissionKind {
    Prompt,
    Steer,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserInputSource {
    pub namespace: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_ref: Option<String>,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    pub display_label_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl UserInputSource {
    pub fn new(
        namespace: impl Into<String>,
        kind: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let namespace = namespace.into();
        let kind = kind.into();
        Self {
            display_label_key: format!("mailbox.source.{namespace}.{kind}"),
            namespace,
            kind,
            source_ref: None,
            correlation_ref: None,
            actor: actor.into(),
            route: None,
            metadata: None,
        }
    }

    pub fn with_route(mut self, route: impl Into<String>) -> Self {
        self.route = Some(route.into());
        self
    }

    pub fn core_composer() -> Self {
        Self::new("core", "composer", "user")
    }

    pub fn companion_parent_resume() -> Self {
        Self::new("companion", "parent_resume", "agent").with_route("parent")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UserInputSubmittedNotification {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub submission_kind: UserInputSubmissionKind,
    pub source: UserInputSource,
    pub content: Vec<codex::UserInput>,
}

impl UserInputSubmittedNotification {
    pub fn new(
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
        item_id: impl Into<String>,
        submission_kind: UserInputSubmissionKind,
        source: UserInputSource,
        content: Vec<codex::UserInput>,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            item_id: item_id.into(),
            submission_kind,
            source,
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

/// 构造一个纯文本 canonical 用户输入块。
///
/// 生产者天然只有文本时（系统触发 prompt、orchestrator 续跑、companion dispatch 等）
/// 用此构造器产出 `Vec<UserInputBlock>`，避免再手写 ACP ContentBlock JSON。
pub fn text_user_input_block(text: impl Into<String>) -> UserInputBlock {
    codex::UserInput::Text {
        text: text.into(),
        text_elements: Vec::new(),
    }
}

/// 把一段文本封装为单元素的 canonical 用户输入向量。
pub fn text_user_input_blocks(text: impl Into<String>) -> Vec<UserInputBlock> {
    vec![text_user_input_block(text)]
}

/// 全项目**唯一**的 `UserInputBlock -> Vec<ContentPart>` 映射。
///
/// 这是 canonical 用户输入（`UserInputBlock`）进入模型层（`ContentPart`）的单一边界，
/// 取代此前散落在 `prompt` / `steer` / `continuation` 三处各自的有损 text 拍平逻辑。
/// 图片走结构化路径（`ContentPart::Image{mime_type,data}`）直达模型，不再拍平成占位文本。
///
/// 各变体处置：
/// - `Text` -> `ContentPart::text`
/// - `Image{url}` -> 解析 data URL（`data:<mime>;base64,<data>`）拆出 `ContentPart::image`；
///   非 data URL 的远程 url 暂保留为文本占位并 `warn`（图片采集侧保证传 data URL）。
/// - `LocalImage{path}` -> 读盘并 base64 编码为 `ContentPart::image`；读不到则降级文本占位并 `warn`。
/// - `Skill` / `Mention` -> 保留 "[引用...]" 文本语义，集中到此唯一定义。
pub fn user_input_blocks_to_content_parts(input: &[UserInputBlock]) -> Vec<ContentPart> {
    input
        .iter()
        .filter_map(user_input_block_to_content_part)
        .collect()
}

fn user_input_block_to_content_part(input: &UserInputBlock) -> Option<ContentPart> {
    match input {
        codex::UserInput::Text { text, .. } => {
            let text = text.trim();
            if text.is_empty() {
                None
            } else {
                Some(ContentPart::text(text))
            }
        }
        codex::UserInput::Image { url, .. } => Some(image_url_to_content_part(url)),
        codex::UserInput::LocalImage { path, .. } => {
            Some(local_image_to_content_part(std::path::Path::new(path)))
        }
        codex::UserInput::Skill { name, path } => Some(ContentPart::text(format!(
            "[引用 Skill: {name} ({})]",
            path
        ))),
        codex::UserInput::Mention { name, path } => {
            Some(ContentPart::text(format!("[引用: {name} ({path})]")))
        }
    }
}

/// 把 image url 转成 `ContentPart`。
///
/// data URL（`data:<mime>;base64,<data>`）拆出 `mime_type` + base64 `data` 直达 `ContentPart::Image`；
/// 非 data URL 的远程 url 暂无法结构化携带，降级为文本占位并告警。
fn image_url_to_content_part(url: &str) -> ContentPart {
    match parse_data_url(url) {
        Some((mime_type, data)) => ContentPart::image(mime_type, data),
        None => {
            diag!(Warn, Subsystem::AgentRun,
                operation = "agent_protocol.user_input",
                stage = "image_url_not_data",
                url_scheme = %url.split_once(':').map(|(scheme, _)| scheme).unwrap_or("unknown"),
                url_length = url.len(),
                "图片 url 非 data URL，无法结构化携带，降级为文本占位（图片采集侧应传 data URL）"
            );
            ContentPart::text(format!("[引用图片: {url}]"))
        }
    }
}

/// 解析 `data:<mime>;base64,<data>`，返回 `(mime_type, base64_data)`。
/// 仅支持 base64 编码的 data URL；非 base64 或非 data URL 返回 `None`。
fn parse_data_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    // meta 形如 `image/png;base64` 或 `image/png`（无 base64 标记则不支持）。
    let mut meta_parts = meta.split(';');
    let mime_type = meta_parts.next().unwrap_or("").trim();
    let is_base64 = meta_parts.any(|part| part.trim().eq_ignore_ascii_case("base64"));
    if mime_type.is_empty() || !is_base64 {
        return None;
    }
    Some((mime_type.to_string(), data.to_string()))
}

/// 读取磁盘图片并 base64 编码为 `ContentPart::Image`；读不到则降级文本占位并告警。
fn local_image_to_content_part(path: &std::path::Path) -> ContentPart {
    use base64::Engine;

    match std::fs::read(path) {
        Ok(bytes) => {
            let mime_type = guess_image_mime(path);
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            ContentPart::image(mime_type, data)
        }
        Err(error) => {
            let diagnostic_context =
                DiagnosticErrorContext::new("agent_protocol.user_input", "local_image_read");
            diag_error!(Warn, Subsystem::AgentRun,
                context = &diagnostic_context,
                error = &error,
                path_extension = %path.extension().and_then(|ext| ext.to_str()).unwrap_or(""),
                path_is_absolute = path.is_absolute(),
                "本地图片读取失败，降级为文本占位"
            );
            ContentPart::text(format!("[引用本地图片: {}]", path.display()))
        }
    }
}

/// 按扩展名粗略推断图片 MIME，未知时回退 `application/octet-stream`。
fn guess_image_mime(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
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

/// **唯一**的"用户输入 -> 文本摘要"投影，**仅供标题提示 / trace 摘要**，不是投递路径。
///
/// 投递路径请用 `user_input_blocks_to_content_parts`（图片结构化直达 `ContentPart::Image`）。
/// 这里把图片/skill/mention 退化为文本仅为生成可读摘要，调用方不得用其结果投递模型。
pub fn codex_user_input_to_text(
    input: &[codex::UserInput],
) -> Result<String, UserInputConversionError> {
    let text = input
        .iter()
        .filter_map(|item| match item {
            codex::UserInput::Text { text, .. } => Some(text.as_str().to_string()),
            codex::UserInput::Image { url, .. } => Some(url.clone()),
            codex::UserInput::LocalImage { path, .. } => Some(path.clone()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_block_maps_to_text_part() {
        let input = vec![codex::UserInput::Text {
            text: "  hello world  ".to_string(),
            text_elements: Vec::new(),
        }];
        let parts = user_input_blocks_to_content_parts(&input);
        assert_eq!(parts, vec![ContentPart::text("hello world")]);
    }

    #[test]
    fn empty_text_block_is_dropped() {
        let input = vec![codex::UserInput::Text {
            text: "   ".to_string(),
            text_elements: Vec::new(),
        }];
        let parts = user_input_blocks_to_content_parts(&input);
        assert!(parts.is_empty());
    }

    #[test]
    fn data_url_image_maps_to_structured_image_part() {
        // 关键证据：data URL 图片真出 ContentPart::Image{mime_type,data}，不再拍平成占位文本。
        let input = vec![codex::UserInput::Image {
            detail: None,
            url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB".to_string(),
        }];
        let parts = user_input_blocks_to_content_parts(&input);
        assert_eq!(
            parts,
            vec![ContentPart::image(
                "image/png",
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB"
            )]
        );
        // 断言确实是结构化 Image，而非文本占位。
        assert!(matches!(parts[0], ContentPart::Image { .. }));
    }

    #[test]
    fn data_url_parse_extracts_mime_and_base64() {
        let parsed = parse_data_url("data:image/jpeg;base64,QUJD");
        assert_eq!(parsed, Some(("image/jpeg".to_string(), "QUJD".to_string())));
    }

    #[test]
    fn data_url_without_base64_marker_is_unsupported() {
        // 非 base64 编码的 data URL 不被结构化解析。
        assert_eq!(parse_data_url("data:image/png,rawtext"), None);
    }

    #[test]
    fn remote_image_url_degrades_to_text_placeholder() {
        let input = vec![codex::UserInput::Image {
            detail: None,
            url: "https://example.com/cat.png".to_string(),
        }];
        let parts = user_input_blocks_to_content_parts(&input);
        assert_eq!(
            parts,
            vec![ContentPart::text("[引用图片: https://example.com/cat.png]")]
        );
    }

    #[test]
    fn mention_block_maps_to_text_reference() {
        let input = vec![codex::UserInput::Mention {
            name: "main.rs".to_string(),
            path: "file://src/main.rs".to_string(),
        }];
        let parts = user_input_blocks_to_content_parts(&input);
        assert_eq!(
            parts,
            vec![ContentPart::text("[引用: main.rs (file://src/main.rs)]")]
        );
    }

    #[test]
    fn mixed_blocks_preserve_order() {
        let input = vec![
            codex::UserInput::Text {
                text: "看这张图".to_string(),
                text_elements: Vec::new(),
            },
            codex::UserInput::Image {
                detail: None,
                url: "data:image/gif;base64,R0lGOD".to_string(),
            },
        ];
        let parts = user_input_blocks_to_content_parts(&input);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], ContentPart::text("看这张图"));
        assert_eq!(parts[1], ContentPart::image("image/gif", "R0lGOD"));
    }

    #[test]
    fn legacy_user_input_envelope_requires_migration_source_backfill() {
        let envelope = crate::BackboneEnvelope::new(
            crate::BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
                "thread-1",
                "turn-1",
                "item-1",
                UserInputSubmissionKind::Prompt,
                UserInputSource::core_composer(),
                vec![codex::UserInput::Text {
                    text: "hello".to_string(),
                    text_elements: Vec::new(),
                }],
            )),
            "session-1",
            crate::SourceInfo {
                connector_id: "test".to_string(),
                connector_type: "test".to_string(),
                executor_id: None,
            },
        );
        let mut legacy_json =
            serde_json::to_value(&envelope).expect("envelope should serialize to json");
        legacy_json["event"]["payload"]
            .as_object_mut()
            .expect("user_input_submitted payload should be an object")
            .remove("source");

        let error = serde_json::from_value::<crate::BackboneEnvelope>(legacy_json.clone())
            .expect_err("missing source should keep protocol data invalid");
        assert!(
            error.to_string().contains("missing field `source`"),
            "unexpected deserialization error: {error}"
        );

        legacy_json["event"]["payload"]["source"] = serde_json::json!({
            "namespace": "core",
            "kind": "composer",
            "actor": "user",
            "displayLabelKey": "mailbox.source.core.composer",
        });
        let migrated = serde_json::from_value::<crate::BackboneEnvelope>(legacy_json)
            .expect("migration backfill should produce a valid user input envelope");
        let crate::BackboneEvent::UserInputSubmitted(input) = migrated.event else {
            panic!("expected user_input_submitted event");
        };

        assert_eq!(input.source, UserInputSource::core_composer());
    }
}

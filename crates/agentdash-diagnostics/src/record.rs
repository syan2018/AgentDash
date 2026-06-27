//! 诊断记录结构与序列化。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 单条结构化诊断记录。
///
/// 落入环形缓冲并由查询端点返回；同样的字段经 tracing fmt JSON 层落地为文件行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticRecord {
    /// 记录时间（Unix epoch 毫秒）。
    pub at_ms: u64,
    /// 级别字符串（`error`/`warn`/`info`/`debug`/`trace`）。
    pub level: String,
    /// 子系统（小写稳定字符串，如 `relay`）。
    pub subsystem: String,
    /// 消息文本。
    pub message: String,
    /// 产生该 event 的 target（通常是模块路径）。
    pub target: String,
    /// 其余结构化字段（不含已抽出的专列与 `subsystem`/`message`）。
    pub fields: Map<String, Value>,
    /// 抽出的关联列：会话 id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// 抽出的关联列：run id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// 抽出的关联列：后端 id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_id: Option<String>,
}

/// 级别的数值序（越大越详细），用于查询时的"最低级别"过滤。
///
/// `error=0, warn=1, info=2, debug=3, trace=4`。
pub(crate) fn level_rank(level: &str) -> u8 {
    match level {
        "error" => 0,
        "warn" => 1,
        "info" => 2,
        "debug" => 3,
        "trace" => 4,
        _ => u8::MAX,
    }
}

/// 把 `tracing::Level` 转为本 crate 使用的小写字符串。
pub(crate) fn level_str(level: &tracing::Level) -> &'static str {
    match *level {
        tracing::Level::ERROR => "error",
        tracing::Level::WARN => "warn",
        tracing::Level::INFO => "info",
        tracing::Level::DEBUG => "debug",
        tracing::Level::TRACE => "trace",
    }
}

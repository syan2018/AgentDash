//! 只读诊断查询端点 —— `GET /api/diagnostics`。
//!
//! 服务**“近期”诊断**：数据来自进程内有界环形缓冲（[`DiagnosticBuffer`]），
//! 容量有限且**进程重启即清空**。完整历史落在 JSON line 滚动日志文件
//! （`AGENTDASH_LOG_DIR`，默认 `./logs/`），本端点不解析文件。
//!
//! 端点 merge 进 `secured_api`，自动套 `authenticate_request` 鉴权，无需单独加中间件。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use agentdash_diagnostics::{DiagnosticFilter, DiagnosticRecord};

use crate::app_state::AppState;

/// `limit` 缺省值。
const DEFAULT_LIMIT: usize = 200;
/// `limit` 上限，防止单次拉取过多。
const MAX_LIMIT: usize = 1000;

/// `GET /api/diagnostics` 查询参数。
#[derive(Debug, Default, Deserialize)]
pub struct DiagnosticsQuery {
    /// 按子系统精确过滤（小写字符串，如 `relay`）。
    pub subsystem: Option<String>,
    /// 按会话 id 精确过滤。
    pub session_id: Option<String>,
    /// 按 run id 精确过滤。
    pub run_id: Option<String>,
    /// 按后端 id 精确过滤。
    pub backend_id: Option<String>,
    /// 最低级别（含），如 `warn` 返回 warn + error。
    pub level: Option<String>,
    /// 仅返回 `at_ms >= since_ms` 的记录。
    pub since_ms: Option<u64>,
    /// 结果条数上限；缺省 [`DEFAULT_LIMIT`]，上限 [`MAX_LIMIT`]。
    pub limit: Option<usize>,
}

impl From<DiagnosticsQuery> for DiagnosticFilter {
    fn from(q: DiagnosticsQuery) -> Self {
        let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
        DiagnosticFilter {
            subsystem: q.subsystem,
            session_id: q.session_id,
            run_id: q.run_id,
            backend_id: q.backend_id,
            min_level: q.level,
            since_ms: q.since_ms,
            limit: Some(limit),
        }
    }
}

/// 查询近期诊断，按时间倒序（最新在前）返回。
pub async fn list_diagnostics(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DiagnosticsQuery>,
) -> Json<Vec<DiagnosticRecord>> {
    let filter: DiagnosticFilter = query.into();
    Json(state.diagnostics.query(&filter))
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new().route("/diagnostics", axum::routing::get(list_diagnostics))
}

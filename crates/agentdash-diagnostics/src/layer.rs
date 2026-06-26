//! `DiagnosticLayer` / `DiagnosticBuffer` —— 有界环形缓冲与查询。

use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

use crate::record::{DiagnosticRecord, level_rank, level_str};

/// 默认环形缓冲容量。
pub const DEFAULT_CAPACITY: usize = 4096;

/// 查询过滤条件。所有字段均为可选；`None` 表示不限制。
#[derive(Debug, Clone, Default)]
pub struct DiagnosticFilter {
    /// 按子系统精确匹配（小写字符串）。
    pub subsystem: Option<String>,
    /// 按会话 id 精确匹配。
    pub session_id: Option<String>,
    /// 按 run id 精确匹配。
    pub run_id: Option<String>,
    /// 按后端 id 精确匹配。
    pub backend_id: Option<String>,
    /// 最低级别（含）：如 `Some("warn")` 返回 error + warn。
    pub min_level: Option<String>,
    /// 仅返回 `at_ms >= since_ms` 的记录。
    pub since_ms: Option<u64>,
    /// 结果条数上限（按时间倒序截断）。
    pub limit: Option<usize>,
}

/// 诊断环形缓冲句柄。可廉价 clone，多处共享同一缓冲。
#[derive(Clone)]
pub struct DiagnosticBuffer {
    inner: Arc<RwLock<VecDeque<DiagnosticRecord>>>,
    capacity: usize,
}

impl DiagnosticBuffer {
    /// 以指定容量新建缓冲（`cap == 0` 时回退到 [`DEFAULT_CAPACITY`]）。
    pub fn new(cap: usize) -> Self {
        let capacity = if cap == 0 { DEFAULT_CAPACITY } else { cap };
        Self {
            inner: Arc::new(RwLock::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    /// 构造一个写入本缓冲的 [`DiagnosticLayer`]。
    pub fn layer(&self) -> DiagnosticLayer {
        DiagnosticLayer {
            buffer: self.clone(),
        }
    }

    /// 当前缓冲内记录条数。
    pub fn len(&self) -> usize {
        self.inner
            .read()
            .expect("diagnostics buffer poisoned")
            .len()
    }

    /// 缓冲是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 按过滤条件查询，按时间倒序（最新在前）返回。
    pub fn query(&self, filter: &DiagnosticFilter) -> Vec<DiagnosticRecord> {
        let guard = self.inner.read().expect("diagnostics buffer poisoned");
        let min_rank = filter.min_level.as_deref().map(level_rank);
        let mut out: Vec<DiagnosticRecord> = guard
            .iter()
            .rev()
            .filter(|r| {
                if let Some(s) = &filter.subsystem
                    && &r.subsystem != s
                {
                    return false;
                }
                if let Some(sid) = &filter.session_id
                    && r.session_id.as_deref() != Some(sid.as_str())
                {
                    return false;
                }
                if let Some(rid) = &filter.run_id
                    && r.run_id.as_deref() != Some(rid.as_str())
                {
                    return false;
                }
                if let Some(bid) = &filter.backend_id
                    && r.backend_id.as_deref() != Some(bid.as_str())
                {
                    return false;
                }
                if let Some(rank) = min_rank
                    && level_rank(&r.level) > rank
                {
                    return false;
                }
                if let Some(since) = filter.since_ms
                    && r.at_ms < since
                {
                    return false;
                }
                true
            })
            .cloned()
            .collect();
        if let Some(limit) = filter.limit
            && out.len() > limit
        {
            out.truncate(limit);
        }
        out
    }

    fn push(&self, record: DiagnosticRecord) {
        let mut guard = self.inner.write().expect("diagnostics buffer poisoned");
        while guard.len() >= self.capacity {
            guard.pop_front();
        }
        guard.push_back(record);
    }
}

/// 写入 [`DiagnosticBuffer`] 的 tracing 层。
pub struct DiagnosticLayer {
    buffer: DiagnosticBuffer,
}

/// 已知的关联列 key，从 event/span 字段抽取进 [`DiagnosticRecord`] 专列。
const SESSION_ID: &str = "session_id";
const RUN_ID: &str = "run_id";
const BACKEND_ID: &str = "backend_id";
const SUBSYSTEM: &str = "subsystem";
const MESSAGE: &str = "message";

/// 字段收集器：把 tracing 字段填进 serde Map，并抽出已知列。
#[derive(Default)]
struct FieldVisitor {
    fields: Map<String, Value>,
    message: Option<String>,
    subsystem: Option<String>,
    session_id: Option<String>,
    run_id: Option<String>,
    backend_id: Option<String>,
}

impl FieldVisitor {
    fn record_string(&mut self, name: &str, value: String) {
        match name {
            MESSAGE => self.message = Some(value),
            SUBSYSTEM => self.subsystem = Some(value),
            SESSION_ID => {
                self.session_id = Some(value.clone());
                self.fields.insert(name.to_string(), Value::String(value));
            }
            RUN_ID => {
                self.run_id = Some(value.clone());
                self.fields.insert(name.to_string(), Value::String(value));
            }
            BACKEND_ID => {
                self.backend_id = Some(value.clone());
                self.fields.insert(name.to_string(), Value::String(value));
            }
            _ => {
                self.fields.insert(name.to_string(), Value::String(value));
            }
        }
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_string(field.name(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_string(field.name(), value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        let name = field.name();
        if name == MESSAGE || name == SUBSYSTEM {
            self.record_string(name, value.to_string());
        } else {
            self.fields.insert(name.to_string(), Value::from(value));
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        let name = field.name();
        if name == MESSAGE || name == SUBSYSTEM {
            self.record_string(name, value.to_string());
        } else {
            self.fields.insert(name.to_string(), Value::from(value));
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), Value::from(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields
            .insert(field.name().to_string(), Value::from(value));
    }
}

/// 存进每个 span 的已知关联列（从 span 字段抽取一次）。
#[derive(Clone, Default)]
struct SpanFields {
    session_id: Option<String>,
    run_id: Option<String>,
    backend_id: Option<String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl<S> Layer<S> for DiagnosticLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        attrs.record(&mut visitor);
        let span_fields = SpanFields {
            session_id: visitor.session_id,
            run_id: visitor.run_id,
            backend_id: visitor.backend_id,
        };
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(span_fields);
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        // 从 event 当前所在的 span 栈中补齐未在 event 上显式给出的关联列。
        if visitor.session_id.is_none() || visitor.run_id.is_none() || visitor.backend_id.is_none()
        {
            if let Some(scope) = ctx.event_scope(event) {
                for span in scope.from_root() {
                    if let Some(sf) = span.extensions().get::<SpanFields>() {
                        if visitor.session_id.is_none() {
                            visitor.session_id = sf.session_id.clone();
                        }
                        if visitor.run_id.is_none() {
                            visitor.run_id = sf.run_id.clone();
                        }
                        if visitor.backend_id.is_none() {
                            visitor.backend_id = sf.backend_id.clone();
                        }
                    }
                }
            }
        }

        let meta = event.metadata();
        let record = DiagnosticRecord {
            at_ms: now_ms(),
            level: level_str(meta.level()).to_string(),
            subsystem: visitor.subsystem.unwrap_or_default(),
            message: visitor.message.unwrap_or_default(),
            target: meta.target().to_string(),
            fields: visitor.fields,
            session_id: visitor.session_id,
            run_id: visitor.run_id,
            backend_id: visitor.backend_id,
        };
        self.buffer.push(record);
    }
}

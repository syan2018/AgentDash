//! # agentdash-diagnostics
//!
//! AgentDash 平台过程诊断的统一 facade。
//!
//! - [`diag!`] 宏：平台过程诊断的唯一入口，强制 `subsystem` 字段，展开为
//!   [`tracing::event!`]（不是 `tracing::info!/warn!/...`），从而与 clippy
//!   `disallowed-macros` 守门共存。
//! - [`Subsystem`]：子系统取值约定。
//! - [`DiagnosticBuffer`] / [`DiagnosticLayer`]：有界环形缓冲 + tracing 层，
//!   供查询端点读取"近期"诊断。
//! - [`DiagnosticRecord`] / [`DiagnosticFilter`]：记录结构与查询条件。
//! - [`diag_error!`] / [`DiagnosticErrorContext`]：统一错误诊断的 operation /
//!   stage / context detail 构造与发射模板。
//!
//! 本 crate 是低层、零业务依赖的库；**不**装配订阅器（不调用 `.init()`），
//! 订阅器装配只在 `agentdash-api` 的 main。

mod diag;
mod layer;
mod macros;
mod record;
mod subsystem;

pub use diag::DiagnosticErrorContext;
pub use layer::{DEFAULT_CAPACITY, DiagnosticBuffer, DiagnosticFilter, DiagnosticLayer};
pub use record::DiagnosticRecord;
pub use subsystem::Subsystem;

/// 供 `diag!` 宏展开内部使用的 `tracing` 重导出。
///
/// 不属于稳定公开 API；存在仅为让 `#[macro_export]` 的 `diag!` 在其它 crate
/// 无需显式 `use tracing` 即可展开。
#[doc(hidden)]
pub use tracing as __tracing;

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    /// 在给定缓冲层下执行闭包，期间产生的 event 落入返回的 buffer。
    fn with_buffer(cap: usize, f: impl FnOnce()) -> DiagnosticBuffer {
        let buffer = DiagnosticBuffer::new(cap);
        let subscriber = tracing_subscriber::registry().with(buffer.layer());
        tracing::subscriber::with_default(subscriber, f);
        buffer
    }

    #[test]
    fn diag_emits_subsystem_message_and_fields() {
        let buffer = with_buffer(16, || {
            let bid = "backend-1";
            diag!(Info, Subsystem::Relay, backend_id = %bid, "后端注册完成");
        });

        let records = buffer.query(&DiagnosticFilter::default());
        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert_eq!(r.subsystem, "relay");
        assert_eq!(r.level, "info");
        assert_eq!(r.message, "后端注册完成");
        assert_eq!(r.backend_id.as_deref(), Some("backend-1"));
        assert_eq!(
            r.fields.get("backend_id").and_then(|v| v.as_str()),
            Some("backend-1")
        );
    }

    #[test]
    fn diag_supports_multiple_fields_and_debug_syntax() {
        let buffer = with_buffer(16, || {
            let sid = "sess-9";
            let count = 3u64;
            diag!(
                Warn,
                Subsystem::SessionLaunch,
                session_id = %sid,
                attempt = count,
                detail = ?Some("x"),
                "启动重试"
            );
        });

        let r = &buffer.query(&DiagnosticFilter::default())[0];
        assert_eq!(r.subsystem, "session_launch");
        assert_eq!(r.level, "warn");
        assert_eq!(r.session_id.as_deref(), Some("sess-9"));
        assert_eq!(r.fields.get("attempt").and_then(|v| v.as_u64()), Some(3));
        assert!(r.fields.contains_key("detail"));
    }

    #[test]
    fn diag_error_emits_standard_error_context_fields() {
        let buffer = with_buffer(16, || {
            let context = DiagnosticErrorContext::new("agent_run.fork", "materialization")
                .with_field("run_id", "run-1")
                .with_field("client_command_id", "cmd-1");
            let error = std::io::Error::new(std::io::ErrorKind::Other, "database exploded");
            diag_error!(
                Error,
                Subsystem::AgentRun,
                context = &context,
                error = &error,
                run_id = "run-1",
                client_command_id = "cmd-1",
                "AgentRun fork failed"
            );
        });

        let r = &buffer.query(&DiagnosticFilter::default())[0];
        assert_eq!(r.subsystem, "agent_run");
        assert_eq!(r.level, "error");
        assert_eq!(r.message, "AgentRun fork failed");
        assert_eq!(
            r.fields.get("operation").and_then(|v| v.as_str()),
            Some("agent_run.fork")
        );
        assert_eq!(
            r.fields.get("stage").and_then(|v| v.as_str()),
            Some("materialization")
        );
        assert!(
            r.fields
                .get("detail")
                .and_then(|v| v.as_str())
                .is_some_and(|value| value.contains("database exploded"))
        );
        assert!(!r.fields.contains_key("diagnostic_context"));
        assert!(
            r.fields
                .get("error")
                .and_then(|v| v.as_str())
                .is_some_and(|value| value.contains("database exploded"))
        );
        assert!(
            r.fields
                .get("error_debug")
                .and_then(|v| v.as_str())
                .is_some_and(|value| value.contains("database exploded"))
        );
    }

    #[test]
    fn query_filters_by_subsystem() {
        let buffer = with_buffer(16, || {
            diag!(Info, Subsystem::Relay, "a");
            diag!(Info, Subsystem::Workflow, "b");
            diag!(Info, Subsystem::Relay, "c");
        });

        let filter = DiagnosticFilter {
            subsystem: Some("relay".to_string()),
            ..Default::default()
        };
        let records = buffer.query(&filter);
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|r| r.subsystem == "relay"));
    }

    #[test]
    fn query_filters_by_min_level() {
        let buffer = with_buffer(16, || {
            diag!(Debug, Subsystem::Infra, "dbg");
            diag!(Info, Subsystem::Infra, "inf");
            diag!(Warn, Subsystem::Infra, "wrn");
            diag!(Error, Subsystem::Infra, "err");
        });

        // 最低级别 warn → 只返回 warn + error。
        let filter = DiagnosticFilter {
            min_level: Some("warn".to_string()),
            ..Default::default()
        };
        let records = buffer.query(&filter);
        assert_eq!(records.len(), 2);
        let levels: Vec<&str> = records.iter().map(|r| r.level.as_str()).collect();
        assert!(levels.contains(&"warn"));
        assert!(levels.contains(&"error"));
        assert!(!levels.contains(&"info"));
    }

    #[test]
    fn query_respects_limit_and_orders_newest_first() {
        let buffer = with_buffer(16, || {
            diag!(Info, Subsystem::Infra, "first");
            diag!(Info, Subsystem::Infra, "second");
            diag!(Info, Subsystem::Infra, "third");
        });

        let filter = DiagnosticFilter {
            limit: Some(2),
            ..Default::default()
        };
        let records = buffer.query(&filter);
        assert_eq!(records.len(), 2);
        // 倒序：最新的 "third" 在前。
        assert_eq!(records[0].message, "third");
        assert_eq!(records[1].message, "second");
    }

    #[test]
    fn query_filters_by_since_ms() {
        let buffer = with_buffer(16, || {
            diag!(Info, Subsystem::Infra, "old");
        });
        // 取一个远未来的时间戳，应过滤掉所有现有记录。
        let future = u64::MAX;
        let filter = DiagnosticFilter {
            since_ms: Some(future),
            ..Default::default()
        };
        assert!(buffer.query(&filter).is_empty());

        // since_ms = 0 应放行全部。
        let filter_all = DiagnosticFilter {
            since_ms: Some(0),
            ..Default::default()
        };
        assert_eq!(buffer.query(&filter_all).len(), 1);
    }

    #[test]
    fn buffer_drops_oldest_when_over_capacity() {
        let buffer = with_buffer(3, || {
            diag!(Info, Subsystem::Infra, "1");
            diag!(Info, Subsystem::Infra, "2");
            diag!(Info, Subsystem::Infra, "3");
            diag!(Info, Subsystem::Infra, "4");
        });

        assert_eq!(buffer.len(), 3);
        let records = buffer.query(&DiagnosticFilter::default());
        let messages: Vec<&str> = records.iter().map(|r| r.message.as_str()).collect();
        // 最早的 "1" 被丢弃；倒序应为 4,3,2。
        assert_eq!(messages, vec!["4", "3", "2"]);
    }

    #[test]
    fn span_fields_propagate_to_event_columns() {
        let buffer = with_buffer(16, || {
            let span = tracing::info_span!("exec", session_id = "sess-span");
            let _e = span.enter();
            // event 自身不带 session_id，应从 span 抽取。
            diag!(Info, Subsystem::AgentRun, "在 span 内");
        });

        let r = &buffer.query(&DiagnosticFilter::default())[0];
        assert_eq!(r.session_id.as_deref(), Some("sess-span"));
    }
}

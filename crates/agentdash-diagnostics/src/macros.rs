//! `diag!` 宏 —— 平台过程诊断的唯一入口。
//!
//! 设计要点：宏展开为 [`tracing::event!`]（**不是** `tracing::info!/warn!/...`），
//! 这样 clippy 的 `disallowed-macros` 封禁裸 `tracing::info!` 等不会误伤本 facade。

/// 平台过程诊断入口宏。
///
/// 签名：`diag!(<Level>, <Subsystem>, <field>=<val>, ..., "message")`
///
/// - `<Level>`：`Error|Warn|Info|Debug|Trace`（映射到 [`tracing::Level`]）。
/// - `<Subsystem>`：[`crate::Subsystem`] 值，必填，渲染为 `subsystem` 字段。
/// - `<field>=<val>`：任意结构化字段，支持 tracing 字段语法（`field = %x` / `field = ?x` / `field = val`）。
///   约定的关联字段为 `session_id` / `run_id` / `backend_id`。
/// - `"message"`：消息字面量（可带格式化参数，同 `tracing::event!`）。
///
/// # 示例
///
/// ```
/// use agentdash_diagnostics::{diag, Subsystem};
/// # let bid = "backend-1";
/// diag!(Info, Subsystem::Relay, backend_id = %bid, "本机后端注册完成，进入消息循环");
/// ```
#[macro_export]
macro_rules! diag {
    // 带字段 + message
    ($level:ident, $subsystem:expr, $($field:tt)+) => {
        $crate::__diag_event!($level, $subsystem, $($field)+)
    };
}

/// 内部宏：把 `diag!` 的 level token 映射到 `tracing::Level` 并展开为 `tracing::event!`。
///
/// 不属于公开 API（`#[doc(hidden)]`），但因 `macro_export` 需对其它 crate 可见。
#[doc(hidden)]
#[macro_export]
macro_rules! __diag_event {
    (Error, $subsystem:expr, $($field:tt)+) => {
        $crate::__diag_emit!($crate::__tracing::Level::ERROR, $subsystem, $($field)+)
    };
    (Warn, $subsystem:expr, $($field:tt)+) => {
        $crate::__diag_emit!($crate::__tracing::Level::WARN, $subsystem, $($field)+)
    };
    (Info, $subsystem:expr, $($field:tt)+) => {
        $crate::__diag_emit!($crate::__tracing::Level::INFO, $subsystem, $($field)+)
    };
    (Debug, $subsystem:expr, $($field:tt)+) => {
        $crate::__diag_emit!($crate::__tracing::Level::DEBUG, $subsystem, $($field)+)
    };
    (Trace, $subsystem:expr, $($field:tt)+) => {
        $crate::__diag_emit!($crate::__tracing::Level::TRACE, $subsystem, $($field)+)
    };
}

/// 内部宏：注入 `subsystem` 字段并展开为 `tracing::event!`。
#[doc(hidden)]
#[macro_export]
macro_rules! __diag_emit {
    ($lvl:expr, $subsystem:expr, $($field:tt)+) => {
        $crate::__tracing::event!(
            $lvl,
            subsystem = %$crate::Subsystem::as_str($subsystem),
            $($field)+
        )
    };
}

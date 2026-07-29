//! AgentDash-owned managed Agent Runtime contract.
//!
//! Complete Agent source commands/reads and Product-facing Runtime wrappers share one contract
//! vocabulary here. Host coordination, persistence and transport framing remain outside this
//! crate.

pub mod canonical_json;
pub mod complete_agent;
#[doc(hidden)]
pub mod encoding_codegen;
pub mod gateway;
pub mod ids;
pub mod runtime_view;
pub mod wire_u64;

pub use canonical_json::*;
pub use complete_agent::*;
pub use gateway::*;
pub use ids::*;
pub use runtime_view::*;
pub use wire_u64::*;

#[cfg(test)]
mod tests {
    use ts_rs::TS;

    use super::*;

    #[test]
    fn runtime_typescript_root_uses_canonical_unsigned_decimal_vocabulary() {
        let typescript = [
            RuntimeU64::decl(),
            AgentSurfaceRevision::decl(),
            AgentSnapshotRevision::decl(),
            AgentObservation::decl(),
            AgentRuntimeView::decl(),
        ]
        .join("\n");

        assert!(!typescript.contains("bigint"));
        for declaration in [
            "type RuntimeU64 = string & { readonly __runtime_u64: \"canonical_unsigned_decimal\" };",
            "type AgentSurfaceRevision = RuntimeU64;",
            "type AgentSnapshotRevision = RuntimeU64;",
            "revision: AgentSnapshotRevision",
            "observation: AgentObservation",
        ] {
            assert!(typescript.contains(declaration), "missing {declaration}");
        }
    }
}

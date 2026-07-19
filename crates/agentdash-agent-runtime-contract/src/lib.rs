//! Application-facing Managed Agent Runtime contract.
//!
//! This crate owns only platform command, snapshot, change, availability, and projection
//! vocabulary. Complete Agent commands and source coordinates belong to
//! `agentdash-agent-service-api`; Host coordination and transport details never cross this
//! boundary.

pub mod canonical_json;
pub mod gateway;
pub mod ids;
pub mod managed_projection;
pub mod presentation;
pub mod wire_u64;

pub use canonical_json::*;
pub use gateway::*;
pub use ids::*;
pub use managed_projection::*;
pub use presentation::*;
pub use wire_u64::*;

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use ts_rs::TS;

    use super::*;

    #[test]
    fn runtime_typescript_root_uses_canonical_unsigned_decimal_vocabulary() {
        let temp = tempfile::tempdir().expect("create TypeScript export directory");
        ManagedRuntimeContractSchema::export_all_to(temp.path())
            .expect("export Managed Runtime contracts");
        RuntimeU64::export_all_to(temp.path()).expect("export Runtime u64");
        let typescript = read_typescript(temp.path());

        assert!(!typescript.contains("bigint"));
        for declaration in [
            "export type RuntimeU64 = string & { readonly __runtime_u64: \"canonical_unsigned_decimal\" };",
            "export type SurfaceRevision = RuntimeU64;",
            "export type RuntimeProjectionRevision = RuntimeU64;",
            "export type RuntimeChangeSequence = RuntimeU64;",
            "captured_at_ms: RuntimeU64",
            "source_change_sequence: RuntimeU64",
        ] {
            assert!(typescript.contains(declaration), "missing {declaration}");
        }
    }

    fn read_typescript(directory: &Path) -> String {
        let mut output = String::new();
        for entry in fs::read_dir(directory).expect("read TypeScript export directory") {
            let path = entry.expect("read TypeScript export entry").path();
            if path.is_dir() {
                output.push_str(&read_typescript(&path));
            } else if path.extension().is_some_and(|extension| extension == "ts") {
                output.push_str(&fs::read_to_string(path).expect("read TypeScript export"));
            }
        }
        output
    }
}

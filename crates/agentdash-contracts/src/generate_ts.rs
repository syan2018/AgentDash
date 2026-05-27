use std::{collections::BTreeMap, env, fs, path::PathBuf};

use agentdash_contracts::extension_package::{
    ExtensionPackageArtifactResponse, ExtensionPackageInstallationResponse,
    InstallExtensionPackageArtifactRequest,
};
use agentdash_contracts::extension_runtime::{
    ExtensionBundleKindResponse, ExtensionBundleProjectionResponse,
    ExtensionCommandHandlerResponse, ExtensionCommandProjectionResponse,
    ExtensionFlagProjectionResponse, ExtensionFlagTypeResponse,
    ExtensionInstallationProjectionResponse, ExtensionInstalledAssetSourceResponse,
    ExtensionMessageRendererDeclarationResponse, ExtensionMessageRendererProjectionResponse,
    ExtensionPackageArtifactRefResponse, ExtensionPermissionAccessResponse,
    ExtensionPermissionDeclarationResponse, ExtensionPermissionProjectionResponse,
    ExtensionRuntimeActionKindResponse, ExtensionRuntimeActionProjectionResponse,
    ExtensionRuntimeInvocationOutputResponse, ExtensionRuntimeInvokeActionRequest,
    ExtensionRuntimeInvokeActionResponse, ExtensionRuntimeProjectionResponse,
    ExtensionRuntimeTraceResponse, ExtensionWorkspaceTabProjectionResponse,
    ExtensionWorkspaceTabRendererResponse, UninstallExtensionInstallationResponse,
};
use agentdash_contracts::mcp_preset::{
    CloneMcpPresetRequest, CreateMcpPresetRequest, ListMcpPresetQuery, McpPresetResponse,
    ProbeMcpPresetResponse, UpdateMcpPresetRequest,
};
use agentdash_contracts::project_agent::{
    CreateProjectAgentRequest, OpenProjectAgentSessionResult, ProjectAgent, ProjectAgentExecutor,
    ProjectAgentSession, ProjectAgentSummary, UpdateProjectAgentRequest,
};
use agentdash_contracts::session::{
    CreateSessionForkRequest, RollbackSessionProjectionRequest, SessionEventResponse,
    SessionEventsPageResponse, SessionForkChildSessionResponse, SessionForkResponse,
    SessionLineageRecordResponse, SessionLineageRelationKindDto, SessionLineageStatusDto,
    SessionLineageViewResponse, SessionMessageRefDto, SessionNdjsonEnvelope,
    SessionProjectionMessageRefResponse, SessionProjectionRollbackResponse,
    SessionProjectionSegmentProvenanceResponse, SessionProjectionSegmentViewResponse,
    SessionProjectionSourceRangeResponse, SessionProjectionViewResponse,
};
use agentdash_contracts::shared_library::{
    InstallLibraryAssetRequest, InstallLibraryAssetResponse, InstalledAssetSourceDto,
    LibraryAssetDto, ListLibraryAssetsQuery, ProjectAssetSourceStatusDto,
    PublishLibraryAssetRequest, SeedBuiltinLibraryAssetsRequest,
};
use agentdash_contracts::vfs::{
    ConfigurableProviderInfo, CreateProjectVfsMountRequest, ListEntriesResponse, ListVfssResponse,
    ProjectVfsMountResponse, ResolveSurfaceRequest, ResolvedVfsSurface, SurfaceApplyPatchRequest,
    SurfaceApplyPatchResponse, SurfaceCreateFileRequest, SurfaceCreateFileResponse,
    SurfaceDeleteFileRequest, SurfaceDeleteFileResponse, SurfaceEntriesResponse,
    SurfaceReadBinaryFileRequest, SurfaceReadFileRequest, SurfaceReadFileResponse,
    SurfaceRenameFileRequest, SurfaceRenameFileResponse, SurfaceStatFileRequest,
    SurfaceStatFileResponse, SurfaceUploadBinaryFileResponse, SurfaceWriteFileRequest,
    SurfaceWriteFileResponse, UpdateProjectVfsMountRequest,
};
use agentdash_contracts::workflow::{
    ActivityDefinition, ActivityLifecycleRunState, ActivityTransition, EffectiveSessionContract,
    LifecycleEdge, LifecycleExecutionEntry, LifecycleRunStatus, LifecycleStepDefinition,
    ValidationIssue, WorkflowBindingKind, WorkflowContract, WorkflowDefinitionSource,
};
use ts_rs::TS;

fn main() {
    let check = env::args().any(|arg| arg == "--check");
    let generated_dir: PathBuf =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/app-web/src/generated");

    write_domain(
        &generated_dir.join("mcp-preset-contracts.ts"),
        &[],
        check,
        |dir| {
            export_all::<McpPresetResponse>(dir);
            export_all::<CreateMcpPresetRequest>(dir);
            export_all::<UpdateMcpPresetRequest>(dir);
            export_all::<CloneMcpPresetRequest>(dir);
            export_all::<ListMcpPresetQuery>(dir);
            export_all::<ProbeMcpPresetResponse>(dir);
        },
    );

    write_domain(
        &generated_dir.join("session-contracts.ts"),
        &["import type { BackboneEnvelope } from \"./backbone-protocol\";"],
        check,
        |dir| {
            export_all::<SessionEventResponse>(dir);
            export_all::<SessionEventsPageResponse>(dir);
            export_all::<SessionNdjsonEnvelope>(dir);
            export_all::<SessionProjectionSourceRangeResponse>(dir);
            export_all::<SessionProjectionMessageRefResponse>(dir);
            export_all::<SessionProjectionSegmentProvenanceResponse>(dir);
            export_all::<SessionProjectionSegmentViewResponse>(dir);
            export_all::<SessionProjectionViewResponse>(dir);
            export_all::<SessionLineageRelationKindDto>(dir);
            export_all::<SessionLineageStatusDto>(dir);
            export_all::<SessionMessageRefDto>(dir);
            export_all::<CreateSessionForkRequest>(dir);
            export_all::<RollbackSessionProjectionRequest>(dir);
            export_all::<SessionLineageRecordResponse>(dir);
            export_all::<SessionForkChildSessionResponse>(dir);
            export_all::<SessionForkResponse>(dir);
            export_all::<SessionLineageViewResponse>(dir);
            export_all::<SessionProjectionRollbackResponse>(dir);
        },
    );

    write_domain(
        &generated_dir.join("workflow-contracts.ts"),
        &[],
        check,
        |dir| {
            export_all::<WorkflowContract>(dir);
            export_all::<ActivityDefinition>(dir);
            export_all::<ActivityTransition>(dir);
            export_all::<ActivityLifecycleRunState>(dir);
            export_all::<LifecycleEdge>(dir);
            export_all::<LifecycleStepDefinition>(dir);
            export_all::<LifecycleExecutionEntry>(dir);
            export_all::<LifecycleRunStatus>(dir);
            export_all::<EffectiveSessionContract>(dir);
            export_all::<ValidationIssue>(dir);
            export_all::<WorkflowBindingKind>(dir);
            export_all::<WorkflowDefinitionSource>(dir);
        },
    );

    write_domain(&generated_dir.join("vfs-contracts.ts"), &[], check, |dir| {
        export_all::<ListVfssResponse>(dir);
        export_all::<ListEntriesResponse>(dir);
        export_all::<ConfigurableProviderInfo>(dir);
        export_all::<ResolvedVfsSurface>(dir);
        export_all::<ResolveSurfaceRequest>(dir);
        export_all::<SurfaceEntriesResponse>(dir);
        export_all::<SurfaceReadFileRequest>(dir);
        export_all::<SurfaceReadFileResponse>(dir);
        export_all::<SurfaceReadBinaryFileRequest>(dir);
        export_all::<SurfaceWriteFileRequest>(dir);
        export_all::<SurfaceWriteFileResponse>(dir);
        export_all::<SurfaceCreateFileRequest>(dir);
        export_all::<SurfaceCreateFileResponse>(dir);
        export_all::<SurfaceDeleteFileRequest>(dir);
        export_all::<SurfaceDeleteFileResponse>(dir);
        export_all::<SurfaceRenameFileRequest>(dir);
        export_all::<SurfaceRenameFileResponse>(dir);
        export_all::<SurfaceStatFileRequest>(dir);
        export_all::<SurfaceStatFileResponse>(dir);
        export_all::<SurfaceApplyPatchRequest>(dir);
        export_all::<SurfaceApplyPatchResponse>(dir);
        export_all::<SurfaceUploadBinaryFileResponse>(dir);
        export_all::<CreateProjectVfsMountRequest>(dir);
        export_all::<UpdateProjectVfsMountRequest>(dir);
        export_all::<ProjectVfsMountResponse>(dir);
    });

    write_domain(
        &generated_dir.join("extension-runtime-contracts.ts"),
        &[],
        check,
        |dir| {
            export_all::<ExtensionRuntimeActionKindResponse>(dir);
            export_all::<ExtensionFlagTypeResponse>(dir);
            export_all::<ExtensionPermissionAccessResponse>(dir);
            export_all::<ExtensionBundleKindResponse>(dir);
            export_all::<ExtensionCommandHandlerResponse>(dir);
            export_all::<ExtensionMessageRendererDeclarationResponse>(dir);
            export_all::<ExtensionWorkspaceTabRendererResponse>(dir);
            export_all::<ExtensionPermissionDeclarationResponse>(dir);
            export_all::<ExtensionInstalledAssetSourceResponse>(dir);
            export_all::<ExtensionPackageArtifactRefResponse>(dir);
            export_all::<ExtensionInstallationProjectionResponse>(dir);
            export_all::<ExtensionCommandProjectionResponse>(dir);
            export_all::<ExtensionFlagProjectionResponse>(dir);
            export_all::<ExtensionMessageRendererProjectionResponse>(dir);
            export_all::<ExtensionRuntimeActionProjectionResponse>(dir);
            export_all::<ExtensionWorkspaceTabProjectionResponse>(dir);
            export_all::<ExtensionPermissionProjectionResponse>(dir);
            export_all::<ExtensionBundleProjectionResponse>(dir);
            export_all::<ExtensionRuntimeProjectionResponse>(dir);
            export_all::<ExtensionRuntimeInvokeActionRequest>(dir);
            export_all::<ExtensionRuntimeTraceResponse>(dir);
            export_all::<ExtensionRuntimeInvocationOutputResponse>(dir);
            export_all::<ExtensionRuntimeInvokeActionResponse>(dir);
            export_all::<UninstallExtensionInstallationResponse>(dir);
        },
    );

    write_domain(
        &generated_dir.join("extension-package-contracts.ts"),
        &[],
        check,
        |dir| {
            export_all::<ExtensionPackageArtifactResponse>(dir);
            export_all::<InstallExtensionPackageArtifactRequest>(dir);
            export_all::<ExtensionPackageInstallationResponse>(dir);
        },
    );

    write_domain(
        &generated_dir.join("shared-library-contracts.ts"),
        &[],
        check,
        |dir| {
            export_all::<InstalledAssetSourceDto>(dir);
            export_all::<LibraryAssetDto>(dir);
            export_all::<ListLibraryAssetsQuery>(dir);
            export_all::<SeedBuiltinLibraryAssetsRequest>(dir);
            export_all::<InstallLibraryAssetRequest>(dir);
            export_all::<InstallLibraryAssetResponse>(dir);
            export_all::<PublishLibraryAssetRequest>(dir);
            export_all::<ProjectAssetSourceStatusDto>(dir);
        },
    );

    write_domain(
        &generated_dir.join("project-agent-contracts.ts"),
        &[],
        check,
        |dir| {
            export_all::<ProjectAgent>(dir);
            export_all::<ProjectAgentExecutor>(dir);
            export_all::<ProjectAgentSession>(dir);
            export_all::<ProjectAgentSummary>(dir);
            export_all::<OpenProjectAgentSessionResult>(dir);
            export_all::<CreateProjectAgentRequest>(dir);
            export_all::<UpdateProjectAgentRequest>(dir);
        },
    );
}

fn write_domain(
    out: &std::path::Path,
    imports: &[&str],
    check: bool,
    export: impl FnOnce(&std::path::Path),
) {
    fs::create_dir_all(out.parent().expect("generated dir")).expect("create generated dir");

    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    export(tmp_dir.path());

    let mut declarations = BTreeMap::new();
    collect_ts_files(tmp_dir.path(), &mut declarations);

    let mut lines = Vec::new();
    lines.push(
        "// This file is generated by `cargo run -p agentdash-contracts --bin generate_contracts_ts`."
            .to_string(),
    );
    lines.push("// Do not edit manually.".to_string());
    lines.push(String::new());

    for import in imports {
        lines.push((*import).to_string());
    }
    if !imports.is_empty() {
        lines.push(String::new());
    }

    for decl in declarations.values() {
        lines.push(decl.clone());
        lines.push(String::new());
    }

    let output = lines.join("\n");

    if check {
        match fs::read_to_string(out) {
            Ok(existing) if existing == output => {
                eprintln!("{} is up to date", out.display());
                return;
            }
            Ok(_) => {
                eprintln!(
                    "{} is out of date; run `cargo run -p agentdash-contracts --bin generate_contracts_ts`",
                    out.display()
                );
                std::process::exit(1);
            }
            Err(error) => {
                eprintln!("failed to read {}: {error}", out.display());
                std::process::exit(1);
            }
        }
    }

    fs::write(out, output).expect("write generated TS");

    eprintln!("Wrote {} ({} types)", out.display(), declarations.len());
}

fn export_all<T: TS + 'static>(dir: &std::path::Path) {
    T::export_all_to(dir).expect("export TS type");
}

fn collect_ts_files(dir: &std::path::Path, out: &mut BTreeMap<String, String>) {
    for entry in fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("read entry");
        let path = entry.path();
        if path.is_dir() {
            collect_ts_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "ts") {
            let content = fs::read_to_string(&path).expect("read ts file");
            let stem = path
                .file_stem()
                .expect("file stem")
                .to_string_lossy()
                .to_string();

            let mut decl_lines = Vec::new();
            for line in content.lines() {
                if line.starts_with("// ") || line.starts_with("import ") {
                    continue;
                }
                if line.is_empty() && decl_lines.is_empty() {
                    continue;
                }
                decl_lines.push(line.trim_end().to_string());
            }

            while decl_lines.last().is_some_and(|l| l.is_empty()) {
                decl_lines.pop();
            }

            if !decl_lines.is_empty() {
                out.insert(stem, decl_lines.join("\n"));
            }
        }
    }
}

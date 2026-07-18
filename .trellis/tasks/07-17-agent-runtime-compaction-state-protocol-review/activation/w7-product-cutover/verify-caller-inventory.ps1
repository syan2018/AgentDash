$ErrorActionPreference = "Stop"

$cutoverRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$workspace = Resolve-Path (Join-Path $cutoverRoot "..\..\..\..\..")
$manifestPath = Join-Path $cutoverRoot "production-cutover-manifest.json"
$manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json

$requiredFields = @(
    "crate",
    "owner",
    "activation_roots",
    "symbols_remove",
    "symbols_add",
    "replacement",
    "prerequisites",
    "activation_gate"
)
if ($manifest.w7_consumer_cut.Count -ne 6) {
    throw "w7_consumer_cut must contain exactly six caller entries"
}
foreach ($entry in $manifest.w7_consumer_cut) {
    foreach ($field in $requiredFields) {
        $value = $entry.$field
        if ($null -eq $value -or ($value -is [string] -and [string]::IsNullOrWhiteSpace($value)) -or
            ($value -is [array] -and $value.Count -eq 0)) {
            throw "$($entry.crate) has an empty $field"
        }
    }
}

$roots = @(
    "crates/agentdash-api/src",
    "crates/agentdash-application/src",
    "crates/agentdash-application-agentrun/src",
    "crates/agentdash-application-lifecycle/src",
    "crates/agentdash-application-ports/src",
    "crates/agentdash-application-vfs/src"
)
$legacyPattern = "AgentRunJournalService|AgentRunJournal|RuntimeJournalFact|RuntimeJournalRecord|RuntimeSession|RuntimeToolProvider|DynAgentTool|AgentTool"
Push-Location $workspace
try {
    $actual = @(rg -l $legacyPattern @roots) |
        ForEach-Object { $_.Replace("\", "/") } |
        Where-Object { $_ -notmatch "/product_protocol/" } |
        Sort-Object -Unique
} finally {
    Pop-Location
}
$expected = @($manifest.w7_consumer_cut.activation_roots) |
    ForEach-Object { ($_ -split "::")[0].Replace("\", "/") } |
    Sort-Object -Unique

$missing = @($actual | Where-Object { $_ -notin $expected })
$stale = @($expected | Where-Object { $_ -notin $actual })
if ($missing.Count -gt 0 -or $stale.Count -gt 0) {
    throw "caller inventory drift; missing=[$($missing -join ', ')]; stale=[$($stale -join ', ')]"
}

$requiredCallerFiles = @(
    "crates/agentdash-api/src/app_state.rs",
    "crates/agentdash-api/src/bootstrap/agent_runtime_surface.rs",
    "crates/agentdash-api/src/routes/lifecycle_agents.rs",
    "crates/agentdash-application/src/task/runtime_tool_provider.rs",
    "crates/agentdash-application/src/wait_activity/provider.rs",
    "crates/agentdash-application/src/runtime_tools/vfs_provider.rs",
    "crates/agentdash-application-agentrun/src/agent_run/journal.rs",
    "crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs",
    "crates/agentdash-application-lifecycle/src/lifecycle/tools/runtime_provider.rs",
    "crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs",
    "crates/agentdash-application-ports/src/runtime_session_delivery.rs",
    "crates/agentdash-application-ports/src/runtime_session_live.rs",
    "crates/agentdash-application-vfs/src/tools/factory.rs",
    "crates/agentdash-application-vfs/src/tools/fs/read.rs"
)
$uncoveredCritical = @($requiredCallerFiles | Where-Object { $_ -notin $expected })
if ($uncoveredCritical.Count -gt 0) {
    throw "critical caller files are not inventoried: $($uncoveredCritical -join ', ')"
}

foreach ($path in @(
    $manifest.api_frontend_generator_cut.api_roots
    $manifest.api_frontend_generator_cut.frontend_roots
    $manifest.api_frontend_generator_cut.generated_roots
)) {
    if (-not (Test-Path (Join-Path $workspace $path))) {
        throw "API/frontend/generated cut path does not exist: $path"
    }
}

Write-Output "caller inventory exactness: passed"

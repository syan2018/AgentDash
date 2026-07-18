param(
    [ValidateSet("Inventory", "Activated")]
    [string]$Mode = "Inventory"
)

$ErrorActionPreference = "Stop"

$cutoverRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$workspace = Resolve-Path (Join-Path $cutoverRoot "..\..\..\..\..")
$manifestPath = Join-Path $cutoverRoot "production-cutover-manifest.json"
$manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json

$requiredGroupFields = @(
    "crate",
    "owner",
    "records"
)
$requiredRecordFields = @(
    "path",
    "owner",
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
    foreach ($field in $requiredGroupFields) {
        $value = $entry.$field
        if ($null -eq $value -or ($value -is [string] -and [string]::IsNullOrWhiteSpace($value)) -or
            ($value -is [array] -and $value.Count -eq 0)) {
            throw "$($entry.crate) has an empty $field"
        }
    }
    foreach ($record in $entry.records) {
        foreach ($field in $requiredRecordFields) {
            $value = $record.$field
            if ($null -eq $value -or
                ($value -is [string] -and [string]::IsNullOrWhiteSpace($value)) -or
                ($field -ne "symbols_remove" -and $value -is [array] -and $value.Count -eq 0)) {
                throw "$($record.path) has an empty $field"
            }
        }
        if ($record.owner -ne $entry.owner) {
            throw "$($record.path) owner does not match $($entry.crate)"
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
$legacyPattern = "AgentRunJournalService|AgentRunJournal|RuntimeJournalFact|RuntimeJournalRecord|RuntimeSession[A-Za-z0-9_]*|RuntimeToolProvider|DynAgentTool|AgentTool[A-Za-z0-9_]*"
Push-Location $workspace
try {
    $actual = @(rg -l $legacyPattern @roots) |
        ForEach-Object { $_.Replace("\", "/") } |
        Where-Object { $_ -notmatch "/product_protocol/" } |
        Sort-Object -Unique

    if ($Mode -eq "Inventory") {
        foreach ($record in $manifest.w7_consumer_cut.records) {
            $actualSymbols = @(& rg -o --no-filename $legacyPattern -- $record.path)
            if ($LASTEXITCODE -notin @(0, 1)) {
                throw "rg failed for $($record.path)"
            }
            $actualSymbols = @($actualSymbols | Sort-Object -Unique)
            $expectedSymbols = @($record.symbols_remove | Sort-Object -Unique)
            $missingSymbols = @($actualSymbols | Where-Object { $_ -notin $expectedSymbols })
            $staleSymbols = @($expectedSymbols | Where-Object { $_ -notin $actualSymbols })
            if ($missingSymbols.Count -gt 0 -or $staleSymbols.Count -gt 0) {
                throw "$($record.path) symbol inventory drift; missing=[$($missingSymbols -join ', ')]; stale=[$($staleSymbols -join ', ')]"
            }
        }
    }
} finally {
    Pop-Location
}
$expected = @($manifest.w7_consumer_cut.records.path) |
    ForEach-Object { $_.Replace("\", "/") } |
    Sort-Object -Unique
$allRecordPaths = @($manifest.w7_consumer_cut.records.path)
if ($allRecordPaths.Count -ne $expected.Count) {
    throw "caller inventory contains duplicate file records"
}

if ($Mode -eq "Inventory") {
    $missing = @($actual | Where-Object { $_ -notin $expected })
    $stale = @($expected | Where-Object { $_ -notin $actual })
    if ($missing.Count -gt 0 -or $stale.Count -gt 0) {
        throw "caller inventory drift; missing=[$($missing -join ', ')]; stale=[$($stale -join ', ')]"
    }
} elseif ($actual.Count -gt 0) {
    throw "activated caller roots still contain legacy symbols: $($actual -join ', ')"
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

foreach ($record in $manifest.api_frontend_generator_cut.records) {
    foreach ($field in @($requiredRecordFields + @("root", "output"))) {
        $value = $record.$field
        if ($null -eq $value -or
            ($value -is [string] -and [string]::IsNullOrWhiteSpace($value)) -or
            ($field -ne "symbols_remove" -and $value -is [array] -and $value.Count -eq 0)) {
            throw "$($record.path) API/frontend/generated record has an empty $field"
        }
    }
    if (-not (Test-Path (Join-Path $workspace $record.path))) {
        throw "API/frontend/generated cut path does not exist: $($record.path)"
    }
    if ($Mode -eq "Inventory" -and $record.path -notlike "crates/agentdash-api/src/*") {
        $content = Get-Content (Join-Path $workspace $record.path) -Raw
        foreach ($symbol in $record.symbols_remove) {
            if ($content -notmatch [regex]::Escape($symbol)) {
                throw "$($record.path) does not contain exact symbols_remove entry $symbol"
            }
        }
    }
}
$projectionRecords = @($manifest.api_frontend_generator_cut.records)
if ($projectionRecords.Count -ne 27) {
    throw "API/frontend/generated cut must contain 14 API, 9 frontend, and 4 generated records"
}
$apiProjectionPaths = @(
    $projectionRecords |
        Where-Object { $_.path -like "crates/agentdash-api/src/*" } |
        ForEach-Object { $_.path } |
        Sort-Object -Unique
)
$apiCallerPaths = @(
    $manifest.w7_consumer_cut |
        Where-Object { $_.crate -eq "agentdash-api" } |
        ForEach-Object { $_.records.path } |
        Sort-Object -Unique
)
if (Compare-Object $apiProjectionPaths $apiCallerPaths) {
    throw "API projection records do not match exact agentdash-api caller records"
}

foreach ($revisionField in @("base_revision", "product_code_commit", "artifact_tip_revision")) {
    $revision = $manifest.$revisionField
    if ($revision -notmatch "^[0-9a-f]{40}$") {
        throw "$revisionField must be a full 40-character git revision"
    }
    Push-Location $workspace
    try {
        & git cat-file -e "$revision`^{commit}"
        if ($LASTEXITCODE -ne 0) {
            throw "$revisionField does not resolve to a commit"
        }
    } finally {
        Pop-Location
    }
}

$lockDelta = $manifest.w8_expected_lock_delta
if ($lockDelta.package -ne "agentdash-application-agentrun" -or
    $lockDelta.dependency_list_add -ne "agentdash-agent-service-api" -or
    $lockDelta.dependency_kind -ne "existing workspace dev-dependency" -or
    $lockDelta.expected_diff_hunk -ne
        "@@ agentdash-application-agentrun dependency list @@`n+ `"agentdash-agent-service-api`"," -or
    $lockDelta.registry_packages_added -ne 0 -or
    $lockDelta.registry_checksums_added -ne 0) {
    throw "W8 expected Cargo.lock delta metadata drifted"
}
$applicationManifest = Get-Content (
    Join-Path $workspace "crates/agentdash-application-agentrun/Cargo.toml"
) -Raw
if ($applicationManifest -notmatch "(?ms)^\[dev-dependencies\].*^agentdash-agent-service-api = \{ workspace = true \}$" -or
    $applicationManifest -match "(?ms)^\[dependencies\].*^agentdash-agent-service-api = \{ workspace = true \}.*^\[dev-dependencies\]") {
    throw "agentdash-agent-service-api must remain an Application dev-dependency only"
}
if ($Mode -eq "Inventory") {
    Push-Location $workspace
    try {
        & git diff --quiet -- Cargo.lock
        if ($LASTEXITCODE -ne 0) {
            throw "Cargo.lock must match HEAD before W8 combined regeneration"
        }
    } finally {
        Pop-Location
    }
}

$sharedFoundation = @(
    $manifest.w8_live_prerequisite_contracts |
        Where-Object { $null -ne $_.consumer_construction }
)
if ($sharedFoundation.Count -ne 1 -or
    "CompanionDispatchCoordinator::new(fork_repository, fresh_repository)" -notin
        $sharedFoundation[0].consumer_construction -or
    @($sharedFoundation[0].consumer_construction | Where-Object {
        $_ -like "CompanionDispatchCoordinator::new*" -and
        $_ -ne "CompanionDispatchCoordinator::new(fork_repository, fresh_repository)"
    }).Count -ne 0) {
    throw "CompanionDispatchCoordinator live construction signature drifted"
}
$companionSource = Get-Content (
    Join-Path $workspace "crates/agentdash-application-agentrun/src/agent_run/product_protocol/companion.rs"
) -Raw
if ($companionSource -notmatch "pub fn new\(\s*fork_repository: &'a dyn AgentRunForkSagaRepository,\s*fresh_repository: &'a dyn CompanionFreshSagaRepository") {
    throw "CompanionDispatchCoordinator source no longer exposes the frozen two-parameter signature"
}

Write-Output "caller inventory $Mode gate: passed"

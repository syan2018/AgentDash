[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$mainRoot = (Resolve-Path (Join-Path $repoRoot '../AgentDash-main-reference')).Path
$expectedCommit = '957fa9d60ea3d67efa1bb278fe5b376cf0c34598'
$fixturePath = Join-Path $repoRoot 'crates/agentdash-agent-runtime-test-support/fixtures/session-parity/main/journal-control.json'
$fixture = Get-Content -Raw $fixturePath | ConvertFrom-Json

if ((git -C $mainRoot rev-parse HEAD).Trim() -ne $expectedCommit) {
    throw 'Pinned Main journal reference commit drifted.'
}
if (-not [string]::IsNullOrWhiteSpace((git -C $mainRoot status --porcelain))) {
    throw 'Pinned Main journal reference must be clean before execution.'
}

foreach ($source in $fixture.provenance.source_files) {
    $sourcePath = Join-Path $mainRoot $source.path
    $actualHash = (Get-FileHash $sourcePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $source.sha256) {
        throw "Pinned Main journal source hash drifted: $($source.path)"
    }
}

$routeSource = Get-Content -Raw (Join-Path $mainRoot 'crates/agentdash-api/src/routes/lifecycle_agents.rs')
$requiredRouteFragments = @(
    'SessionNdjsonEnvelope::connected(',
    'stream_state.connected_seq',
    'stream_state.ephemeral_epoch',
    'Err(tokio::sync::broadcast::error::RecvError::Lagged(n))',
    'continue;',
    'Err(tokio::sync::broadcast::error::RecvError::Closed)',
    'break;',
    'SessionNdjsonEnvelope::heartbeat_now()'
)
foreach ($fragment in $requiredRouteFragments) {
    if (-not $routeSource.Contains($fragment)) {
        throw "Pinned Main journal production stream route no longer contains required behavior: $fragment"
    }
}

$previousTarget = $env:CARGO_TARGET_DIR
try {
    $env:CARGO_TARGET_DIR = Join-Path $repoRoot 'target'
    & cargo test --locked --manifest-path (Join-Path $mainRoot 'Cargo.toml') -p agentdash-application-agentrun agent_run::journal::tests -- --nocapture
    if ($LASTEXITCODE -ne 0) {
        throw 'Pinned Main AgentRun journal production tests failed.'
    }
    & cargo run --offline --locked --manifest-path (Join-Path $PSScriptRoot 'pinned-journal-capture/Cargo.toml')
    if ($LASTEXITCODE -ne 0) {
        throw 'Pinned Main AgentRun journal control capture failed.'
    }
}
finally {
    $env:CARGO_TARGET_DIR = $previousTarget
}

if (-not [string]::IsNullOrWhiteSpace((git -C $mainRoot status --porcelain))) {
    throw 'Pinned Main journal execution modified the oracle worktree.'
}

Write-Output 'Pinned Main AgentRun fork/heartbeat/lagged/closed production evidence verified.'

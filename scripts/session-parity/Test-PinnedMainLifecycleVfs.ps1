[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$mainRoot = (Resolve-Path (Join-Path $repoRoot '../AgentDash-main-reference')).Path
$expectedCommit = '957fa9d60ea3d67efa1bb278fe5b376cf0c34598'
$captureManifest = Join-Path $PSScriptRoot 'pinned-lifecycle-vfs-capture/Cargo.toml'

if ((git -C $mainRoot rev-parse HEAD).Trim() -ne $expectedCommit) {
    throw 'Pinned Main Lifecycle VFS reference commit drifted.'
}
if (-not [string]::IsNullOrWhiteSpace((git -C $mainRoot status --porcelain))) {
    throw 'Pinned Main Lifecycle VFS reference must be clean before execution.'
}

$previousTarget = $env:CARGO_TARGET_DIR
try {
    $env:CARGO_TARGET_DIR = Join-Path $repoRoot 'target'
    & cargo run --offline --locked --manifest-path $captureManifest
    if ($LASTEXITCODE -ne 0) {
        throw 'Pinned Main Lifecycle VFS observable capture failed.'
    }
}
finally {
    $env:CARGO_TARGET_DIR = $previousTarget
}

if (-not [string]::IsNullOrWhiteSpace((git -C $mainRoot status --porcelain))) {
    throw 'Pinned Main Lifecycle VFS capture modified the oracle worktree.'
}

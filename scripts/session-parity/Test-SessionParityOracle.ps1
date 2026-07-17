[CmdletBinding()]
param(
    [string]$ManifestPath = (Join-Path $PSScriptRoot 'oracle-manifest.json')
)

$ErrorActionPreference = 'Stop'

function Invoke-Git {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Repository,
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    $output = & git -C $Repository @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "git -C '$Repository' $($Arguments -join ' ') failed: $output"
    }
    return ($output | Out-String).Trim()
}

function Get-NormalizedFileSha256 {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $normalized = New-Object 'System.Collections.Generic.List[byte]' $bytes.Length
    for ($index = 0; $index -lt $bytes.Length; $index++) {
        if ($bytes[$index] -eq 13 -and $index + 1 -lt $bytes.Length -and $bytes[$index + 1] -eq 10) {
            $normalized.Add(10)
            $index++
            continue
        }
        $normalized.Add($bytes[$index])
    }

    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        return (($sha256.ComputeHash($normalized.ToArray()) | ForEach-Object { $_.ToString('x2') }) -join '')
    }
    finally {
        $sha256.Dispose()
    }
}

$manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
$workspaceRoot = Invoke-Git -Repository $PSScriptRoot -Arguments @('rev-parse', '--show-toplevel')
$referencePath = [System.IO.Path]::GetFullPath(
    (Join-Path $workspaceRoot ([string]$manifest.reference_path))
)
if (-not (Test-Path -LiteralPath $referencePath -PathType Container)) {
    throw "Main oracle path does not exist: $referencePath"
}

$actualHead = Invoke-Git -Repository $referencePath -Arguments @('rev-parse', 'HEAD')
if ($actualHead -ne [string]$manifest.oracle_commit) {
    throw "Main oracle HEAD drifted: expected $($manifest.oracle_commit), actual $actualHead"
}

$referenceStatus = Invoke-Git -Repository $referencePath -Arguments @('status', '--porcelain=v1', '--untracked-files=all')
if ($referenceStatus) {
    throw "Main oracle is not read-only clean:`n$referenceStatus"
}

foreach ($source in $manifest.source_files) {
    $sourcePath = Join-Path $workspaceRoot ([string]$source.path)
    if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) {
        throw "Current canonical source is missing: $($source.path)"
    }
    $actualHash = Get-NormalizedFileSha256 -Path $sourcePath
    if ($actualHash -ne [string]$source.sha256) {
        throw "Current canonical source drifted: $($source.path) expected $($source.sha256), actual $actualHash"
    }
}

foreach ($absentPath in $manifest.absent_legacy_paths) {
    if (Test-Path -LiteralPath (Join-Path $workspaceRoot ([string]$absentPath))) {
        throw "Retired legacy owner path reappeared outside the canonical Runtime boundary: $absentPath"
    }
}
$nestedCargoTargets = @(
    'scripts/session-parity/pinned-journal-capture/target',
    'scripts/session-parity/pinned-lifecycle-vfs-capture/target'
)
foreach ($relativeTarget in $nestedCargoTargets) {
    $nestedTarget = Join-Path $workspaceRoot $relativeTarget
    if (Test-Path -LiteralPath $nestedTarget) {
        throw "Pinned Main runner must use the shared workspace Cargo target: $relativeTarget"
    }
}
& git -C $workspaceRoot merge-base --is-ancestor ([string]$manifest.task_start_commit) HEAD
if ($LASTEXITCODE -ne 0) {
    throw "Current HEAD is not descended from W0 task start $($manifest.task_start_commit)"
}

foreach ($harness in $manifest.harness_files) {
    $harnessPath = Join-Path $workspaceRoot ([string]$harness.path)
    if (-not (Test-Path -LiteralPath $harnessPath -PathType Leaf)) {
        throw "Session parity harness file is missing: $($harness.path)"
    }
    $actualHash = Get-NormalizedFileSha256 -Path $harnessPath
    if ($actualHash -ne [string]$harness.sha256) {
        throw "Session parity harness drifted: $($harness.path) expected $($harness.sha256), actual $actualHash"
    }
}

[pscustomobject]@{
    oracle_commit = $actualHead
    oracle_clean = $true
    verified_source_files = $manifest.source_files.Count
    verified_harness_files = $manifest.harness_files.Count
    current_head = Invoke-Git -Repository $workspaceRoot -Arguments @('rev-parse', 'HEAD')
    task_start_commit = [string]$manifest.task_start_commit
}

$ErrorActionPreference = "Stop"

$base = "fc26d3ffb951461d8e9214b6b4639b88c18d533d"
$taskRelative = ".trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review"
$components = @(
    @{
        Name = "platform_runtime"
        Worktree = "F:\Projects\AgentDash-s5-platform-activation"
        Tip = "30d9a55597e36fc5af0591c420346c3217c1dbae"
        Artifact = "$taskRelative/activation/w3-platform-runtime/manifest.json"
    },
    @{
        Name = "dash_native"
        Worktree = "F:\Projects\AgentDash-s5-dash-activation"
        Tip = "6c38dd3de7527859f21e21b28a6b7cb37c7e0f5c"
        Artifact = "$taskRelative/activation/w2-dash-core/consumer-manifest.json"
    },
    @{
        Name = "external_agents"
        Worktree = "F:\Projects\AgentDash-s5-external-activation"
        Tip = "ffaf54a749659923e28599fe075616d34c292b43"
        Artifact = "$taskRelative/activation/w6-external-agents/manifest.json"
    },
    @{
        Name = "product_protocol"
        Worktree = "F:\Projects\AgentDash-s5-product-activation"
        Tip = "67d9eef5f078dcb10077bbdb2eab1a05d2a33674"
        Artifact = "$taskRelative/activation/w7-product-cutover/production-cutover-manifest.json"
    }
)

$filesByComponent = @{}

foreach ($component in $components) {
    if (-not (Test-Path -LiteralPath $component.Worktree)) {
        throw "missing worktree: $($component.Worktree)"
    }

    $actualTip = (& git -C $component.Worktree rev-parse HEAD).Trim()
    if ($actualTip -ne $component.Tip) {
        throw "$($component.Name) tip mismatch: expected $($component.Tip), got $actualTip"
    }

    $status = (& git -C $component.Worktree status --porcelain)
    if ($status) {
        throw "$($component.Name) worktree is not clean"
    }

    & git -C $component.Worktree merge-base --is-ancestor $base $component.Tip
    if ($LASTEXITCODE -ne 0) {
        throw "$($component.Name) does not descend from frozen base"
    }

    $lockDiff = (& git -C $component.Worktree diff --name-only "$base..$($component.Tip)" -- Cargo.lock)
    if ($lockDiff) {
        throw "$($component.Name) modified Cargo.lock"
    }

    $artifactPath = Join-Path $component.Worktree $component.Artifact
    if (-not (Test-Path -LiteralPath $artifactPath)) {
        throw "$($component.Name) artifact missing: $artifactPath"
    }
    Get-Content -Raw -LiteralPath $artifactPath | ConvertFrom-Json | Out-Null

    $files = @(& git -C $component.Worktree diff --name-only "$base..$($component.Tip)")
    $filesByComponent[$component.Name] = $files
}

$actualOverlaps = @()
for ($leftIndex = 0; $leftIndex -lt $components.Count; $leftIndex++) {
    for ($rightIndex = $leftIndex + 1; $rightIndex -lt $components.Count; $rightIndex++) {
        $left = $components[$leftIndex].Name
        $right = $components[$rightIndex].Name
        $rightSet = [System.Collections.Generic.HashSet[string]]::new(
            [string[]]$filesByComponent[$right],
            [System.StringComparer]::Ordinal
        )
        foreach ($path in $filesByComponent[$left]) {
            if ($rightSet.Contains($path)) {
                $actualOverlaps += "$left|$right|$path"
            }
        }
    }
}

$expectedOverlaps = @(
    "dash_native|product_protocol|crates/agentdash-application-agentrun/Cargo.toml"
)

$actualSorted = @($actualOverlaps | Sort-Object)
$expectedSorted = @($expectedOverlaps | Sort-Object)
if (($actualSorted -join "`n") -ne ($expectedSorted -join "`n")) {
    throw "component overlap mismatch: $($actualSorted -join ', ')"
}

$hardCutWorktree = "F:\Projects\AgentDash-s5-hard-cut"
$hardCutTip = (& git -C $hardCutWorktree rev-parse HEAD).Trim()
if ($hardCutTip -ne $base) {
    throw "hard-cut worktree no longer points at frozen base"
}
if (& git -C $hardCutWorktree status --porcelain) {
    throw "hard-cut worktree is not clean"
}

[ordered]@{
    result = "pass"
    frozen_base_sha = $base
    component_count = $components.Count
    expected_overlap_count = $expectedOverlaps.Count
    hard_cut_tip = $hardCutTip
} | ConvertTo-Json

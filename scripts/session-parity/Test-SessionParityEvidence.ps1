[CmdletBinding()]
param(
    [switch]$FinalComplete
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$fixtureRoot = Join-Path $repoRoot 'crates/agentdash-agent-runtime-test-support/fixtures/session-parity'
$catalog = Get-Content -Raw (Join-Path $fixtureRoot 'scenario-catalog.json') | ConvertFrom-Json
$manifest = Get-Content -Raw (Join-Path $fixtureRoot 'evidence-manifest.json') | ConvertFrom-Json

Push-Location $repoRoot
try {
    & (Join-Path $PSScriptRoot 'Test-SessionParityOracle.ps1')
    if ($LASTEXITCODE -ne 0) {
        throw 'Pinned Main oracle preflight failed.'
    }

    if ($FinalComplete) {
        if ($manifest.mode -ne 'final_complete') {
            throw "Final-complete gate requires evidence manifest mode=final_complete; found $($manifest.mode)."
        }
        $inventory = Get-Content -Raw (Join-Path $fixtureRoot 'inventory.json') | ConvertFrom-Json
        if ($inventory.status -ne 'final_complete') {
            throw "Final-complete gate requires inventory status=final_complete; found $($inventory.status)."
        }
    }

    & cargo test -p agentdash-agent-runtime-test-support central_evidence_manifest_is_truthful_and_complete
    if ($LASTEXITCODE -ne 0) {
        throw 'Central session-parity evidence validation failed.'
    }

    $evidenceById = @{}
    foreach ($evidence in $manifest.evidence) {
        if ($evidenceById.ContainsKey($evidence.id)) {
            throw "Duplicate session-parity evidence id: $($evidence.id)"
        }
        $evidenceById[$evidence.id] = $evidence
    }
    $completionGroups = @{}
    foreach ($group in $manifest.completion_groups) {
        $completionGroups[$group.scenario] = @($group.evidence_ids)
    }
    $extensionGroups = @{}
    foreach ($group in $manifest.extension_groups) {
        if ($extensionGroups.ContainsKey($group.scenario)) {
            throw "Duplicate extension evidence group: $($group.scenario)"
        }
        $extensionGroups[$group.scenario] = $group
    }

    $completionErrors = [System.Collections.Generic.List[string]]::new()
    foreach ($scenario in $catalog.scenarios) {
        $hasCompleteEvidence = $false
        foreach ($evidenceId in @($scenario.evidence_ids)) {
            if (-not $evidenceById.ContainsKey($evidenceId)) {
                $completionErrors.Add("$($scenario.id): missing evidence $evidenceId")
                continue
            }
            if (@($evidenceById[$evidenceId].complete_scenarios) -contains $scenario.id) {
                $hasCompleteEvidence = $true
            }
        }
        if ($completionGroups.ContainsKey($scenario.id)) {
            $groupComplete = $true
            foreach ($evidenceId in $completionGroups[$scenario.id]) {
                if (-not $evidenceById.ContainsKey($evidenceId)) {
                    $groupComplete = $false
                    continue
                }
                $covered = @($evidenceById[$evidenceId].complete_scenarios) -contains $scenario.id
                $covered = $covered -or (@($evidenceById[$evidenceId].partial_scenarios) -contains $scenario.id)
                if (-not $covered) {
                    $groupComplete = $false
                }
            }
            $hasCompleteEvidence = $hasCompleteEvidence -or $groupComplete
        }
        if ($scenario.status -eq 'golden_verified' -and -not $hasCompleteEvidence) {
            $completionErrors.Add("$($scenario.id): no complete strict evidence")
        }
        if ($scenario.status -eq 'planned' -and $hasCompleteEvidence) {
            $completionErrors.Add("$($scenario.id): complete evidence is still marked planned")
        }
        if ($scenario.status -eq 'extension_verified') {
            if (-not $extensionGroups.ContainsKey($scenario.id)) {
                $completionErrors.Add("$($scenario.id): extension_verified requires a three-proof extension group")
            }
            else {
                $extensionGroup = $extensionGroups[$scenario.id]
                $roles = @(
                    @('main_absence_evidence_id', 'pinned_main_absence'),
                    @('current_typed_execution_evidence_id', 'current_typed_execution'),
                    @('protected_surface_evidence_id', 'protected_main_surface_unchanged')
                )
                foreach ($role in $roles) {
                    $evidenceId = $extensionGroup.($role[0])
                    if (-not $evidenceById.ContainsKey($evidenceId)) {
                        $completionErrors.Add("$($scenario.id): missing extension evidence $evidenceId")
                        continue
                    }
                    $evidence = $evidenceById[$evidenceId]
                    if ($evidence.strength -ne $role[1]) {
                        $completionErrors.Add("$($scenario.id): extension evidence $evidenceId has invalid strength $($evidence.strength)")
                    }
                    if (-not (@($evidence.partial_scenarios) -contains $scenario.id)) {
                        $completionErrors.Add("$($scenario.id): extension evidence $evidenceId does not cover scenario")
                    }
                }
            }
        }
        if ($FinalComplete -and $scenario.status -eq 'planned') {
            $completionErrors.Add("$($scenario.id): remains planned")
        }
    }
    if ($completionErrors.Count -gt 0) {
        throw "Session-parity completion gate failed:`n$($completionErrors -join "`n")"
    }

    $seenCommands = @{}
    foreach ($evidence in $manifest.evidence) {
        $cargoArgsProperty = $evidence.PSObject.Properties['cargo_args']
        $commandProperty = $evidence.PSObject.Properties['command']
        if ($null -ne $cargoArgsProperty -and @($cargoArgsProperty.Value).Count -gt 0) {
            $executable = 'cargo'
            $arguments = @($cargoArgsProperty.Value)
        }
        elseif ($null -ne $commandProperty -and @($commandProperty.Value).Count -gt 0) {
            $executable = $commandProperty.Value[0]
            $arguments = @($commandProperty.Value | Select-Object -Skip 1)
        }
        else {
            throw "Session-parity evidence has no executable command: $($evidence.id)"
        }
        $commandKey = (@($executable) + $arguments) -join "`u{001f}"
        if ($seenCommands.ContainsKey($commandKey)) {
            continue
        }
        $seenCommands[$commandKey] = $true
        Write-Host "Running evidence: $($evidence.id)"
        & $executable @arguments
        if ($LASTEXITCODE -ne 0) {
            throw "Session-parity evidence failed: $($evidence.id)"
        }
    }

    & (Join-Path $PSScriptRoot 'Test-CurrentRuntimePresentationMigration.ps1')
    if ($LASTEXITCODE -ne 0) {
        throw 'Current Runtime presentation migration exact evidence failed.'
    }

    & (Join-Path $PSScriptRoot 'Test-PinnedMainJournal.ps1')
    if ($LASTEXITCODE -ne 0) {
        throw 'Pinned Main journal production tests failed.'
    }

    $planned = @($catalog.scenarios | Where-Object status -eq 'planned' | ForEach-Object id)
    if ($planned.Count -gt 0) {
        Write-Host "Incremental evidence passed; planned scenarios remain: $($planned -join ', ')"
    }
    else {
        Write-Host 'All session-parity scenarios have executable strict evidence.'
    }
}
finally {
    Pop-Location
}

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path

Push-Location $repoRoot
try {
    $testName = 'persistence::postgres::runtime_repository::tests::presentation_contract_upgrade_clears_runtime_graph_without_rewriting_migration_history'
    $testOutput = (& cmd.exe /d /s /c "cargo test -p agentdash-infrastructure $testName -- --exact --nocapture 2>&1" | Out-String)
    $testExitCode = $LASTEXITCODE
    Write-Output $testOutput
    if ($testExitCode -ne 0 -or $testOutput -notmatch 'test result: ok\. 1 passed;') {
        throw "Current Runtime presentation migration exact test failed: $testName"
    }
}
finally {
    Pop-Location
}

Write-Output 'Current Runtime presentation migration exact evidence verified.'

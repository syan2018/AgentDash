[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path

Push-Location $repoRoot
try {
    $applicationTests = @(
        'agent_run::journal::tests::fork_get_and_reconnect_share_one_ordered_projection',
        'agent_run::journal::tests::multi_level_fork_applies_each_parent_local_cutoff_before_concatenation',
        'agent_run::journal::tests::future_resume_cursor_is_accepted_and_ephemeral_sequence_uses_transient_coordinate',
        'agent_run::journal::tests::future_connected_cursor_does_not_renumber_next_durable_live_event'
    )
    foreach ($testName in $applicationTests) {
        $testOutput = (& cmd.exe /d /s /c "cargo test -p agentdash-application-agentrun $testName -- --exact --nocapture 2>&1" | Out-String)
        $testExitCode = $LASTEXITCODE
        Write-Output $testOutput
        if ($testExitCode -ne 0 -or $testOutput -notmatch 'test result: ok\. 1 passed;') {
            throw "Current AgentRun journal production test failed: $testName"
        }
    }

    $routeTest = 'routes::lifecycle_agents::journal_projection_tests::journal_controls_and_retention_gap_match_fixed_main_control_golden'
    $routeOutput = (& cmd.exe /d /s /c "cargo test -p agentdash-api $routeTest -- --exact --nocapture 2>&1" | Out-String)
    $routeExitCode = $LASTEXITCODE
    Write-Output $routeOutput
    if ($routeExitCode -ne 0 -or $routeOutput -notmatch 'test result: ok\. 1 passed;') {
        throw "Current AgentRun journal production route test failed: $routeTest"
    }
}
finally {
    Pop-Location
}

Write-Output 'Current AgentRun fork/heartbeat/lagged/closed production evidence verified.'

param(
    [string]$EnvFile = "deploy/compose/.env",
    [string[]]$ComposeFile = @("deploy/compose/docker-compose.yml"),
    [string]$Version,
    [string]$ImageRepository,
    [switch]$ManagedPostgres,
    [switch]$SkipBackup,
    [switch]$SkipPull,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")

function Resolve-PathIfExists([string]$PathValue) {
    $candidate = if ([System.IO.Path]::IsPathRooted($PathValue)) {
        $PathValue
    } else {
        Join-Path $repoRoot $PathValue
    }
    if (-not (Test-Path -LiteralPath $candidate)) {
        throw "路径不存在: $PathValue"
    }
    return (Resolve-Path -LiteralPath $candidate).Path
}

function Read-EnvFile([string]$PathValue) {
    $result = @{}
    foreach ($line in Get-Content -LiteralPath $PathValue) {
        $trimmed = $line.Trim()
        if (-not $trimmed -or $trimmed.StartsWith("#")) {
            continue
        }
        $parts = $trimmed.Split("=", 2)
        if ($parts.Count -ne 2) {
            continue
        }
        $result[$parts[0].Trim()] = $parts[1].Trim()
    }
    return $result
}

function First-NonEmpty {
    foreach ($value in $args) {
        if ($null -ne $value -and "$value".Trim().Length -gt 0) {
            return "$value"
        }
    }
    return $null
}

function Compose-BaseArgs {
    $args = @("compose")
    foreach ($file in $composeFiles) {
        $args += @("-f", $file)
    }
    $args += @("--env-file", $envFilePath)
    return $args
}

function Format-Command([string]$Command, [string[]]$CommandArgs) {
    $parts = @($Command) + $CommandArgs
    return ($parts | ForEach-Object {
        if ($_ -match "\s") {
            '"' + ($_ -replace '"', '\"') + '"'
        } else {
            $_
        }
    }) -join " "
}

function Invoke-CommandStep([string]$Command, [string[]]$CommandArgs) {
    Write-Host "[run] $(Format-Command $Command $CommandArgs)"
    if ($DryRun) {
        return
    }
    & $Command @CommandArgs
    if ($LASTEXITCODE -ne 0) {
        throw "$Command failed with exit code $LASTEXITCODE"
    }
}

function Invoke-Compose([string[]]$ComposeArgs) {
    Invoke-CommandStep -Command "docker" -CommandArgs ((Compose-BaseArgs) + $ComposeArgs)
}

function Invoke-PostgresBackup {
    $timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $backupDir = Join-Path $repoRoot "deploy/compose/backups"
    $backupFile = Join-Path $backupDir "agentdash-$timestamp.dump"
    $containerFile = "/tmp/agentdash-$timestamp.dump"
    if (-not $DryRun) {
        New-Item -ItemType Directory -Force -Path $backupDir | Out-Null
    }

    Write-Host "[deploy] backup: $backupFile"
    $dumpCommand = 'pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB" --format=custom --no-owner --file=' + $containerFile
    Invoke-Compose @("exec", "-T", "postgres", "sh", "-c", $dumpCommand)
    Invoke-Compose @("cp", "postgres:$containerFile", $backupFile)
    Invoke-Compose @("exec", "-T", "postgres", "rm", "-f", $containerFile)
}

function Invoke-HttpCheck([string]$Url) {
    Write-Host "[check] $Url"
    if ($DryRun) {
        return
    }
    $response = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 20
    if ($response.StatusCode -lt 200 -or $response.StatusCode -ge 300) {
        throw "HTTP check failed: $Url -> $($response.StatusCode)"
    }
}

$envFilePath = Resolve-PathIfExists $EnvFile
$composeFiles = @()
foreach ($file in $ComposeFile) {
    $composeFiles += Resolve-PathIfExists $file
}
if ($ManagedPostgres) {
    $managedFile = Resolve-PathIfExists "deploy/compose/docker-compose.managed-postgres.yml"
    if ($composeFiles -notcontains $managedFile) {
        $composeFiles += $managedFile
    }
    if (-not $SkipBackup) {
        throw "Managed PostgreSQL 模式无法通过 Compose postgres 容器执行备份；请先完成外部数据库快照，然后追加 -SkipBackup。"
    }
}

$envValues = Read-EnvFile $envFilePath
$targetVersion = First-NonEmpty $Version $envValues["AGENTDASH_VERSION"]
$targetImageRepository = First-NonEmpty $ImageRepository $envValues["AGENTDASH_IMAGE_REPOSITORY"] "agentdash-cloud"
$publicOrigin = (First-NonEmpty $envValues["AGENTDASH_PUBLIC_ORIGIN"] "http://127.0.0.1:8080").TrimEnd("/")

if (-not $targetVersion) {
    throw "缺少 AGENTDASH_VERSION；请在 env file 中配置或传入 -Version。"
}

$oldVersion = $env:AGENTDASH_VERSION
$oldImageRepository = $env:AGENTDASH_IMAGE_REPOSITORY
try {
    $env:AGENTDASH_VERSION = $targetVersion
    $env:AGENTDASH_IMAGE_REPOSITORY = $targetImageRepository

    Write-Host "[deploy] version: $targetVersion"
    Write-Host "[deploy] image: ${targetImageRepository}:$targetVersion"
    Write-Host "[deploy] env: $envFilePath"
    Write-Host "[deploy] mode: $(if ($ManagedPostgres) { 'managed-postgres' } else { 'compose-postgres' })"

    Invoke-Compose @("config")

    if (-not $SkipPull) {
        Invoke-Compose @("pull", "migrate", "agentdash-cloud", "reverse-proxy")
    } else {
        Write-Host "[deploy] skip pull"
    }

    if (-not $SkipBackup) {
        Invoke-PostgresBackup
    } else {
        Write-Host "[deploy] skip backup"
    }

    Invoke-Compose @("run", "--rm", "migrate")
    Invoke-Compose @("up", "-d", "agentdash-cloud", "reverse-proxy")
    Invoke-HttpCheck "$publicOrigin/api/health"
    Invoke-HttpCheck "$publicOrigin/api/version"
    Invoke-Compose @("run", "--rm", "agentdash-cloud", "doctor")

    Write-Host "[deploy] update completed"
} finally {
    $env:AGENTDASH_VERSION = $oldVersion
    $env:AGENTDASH_IMAGE_REPOSITORY = $oldImageRepository
}

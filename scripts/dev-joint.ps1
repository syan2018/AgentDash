<#
.SYNOPSIS
  AgentDash 联合启动脚本 — 保序拉起 server → local → frontend
.DESCRIPTION
  启动顺序（基于健康检查，不是盲等）：
    1. 构建 Rust 二进制（server + local）
    2. 启动 agentdash-server (:3001)，轮询 /api/health 直到就绪
    3. 启动 agentdash-local（WS 连接到 server）
    4. 启动 frontend Vite dev server (:5173)
  按 Ctrl+C 统一停止。
.EXAMPLE
  .\scripts\dev-joint.ps1
  .\scripts\dev-joint.ps1 -AccessibleRoots "D:\Project1,D:\Project2"
  .\scripts\dev-joint.ps1 -NoExecutor -SkipBuild
#>
param(
    [string]$AccessibleRoots = "",
    [switch]$NoExecutor,
    [switch]$SkipBuild,
    [string]$BackendId = "local-dev-1",
    [string]$BackendName = "dev-local",
    [int]$ServerPort = 3001
)

$ErrorActionPreference = "Stop"

# ── 定位项目根 ──
$root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path "$root\Cargo.toml")) { $root = Get-Location }

if ([string]::IsNullOrWhiteSpace($AccessibleRoots)) {
    $AccessibleRoots = $root.ToString()
}

Write-Host ""
Write-Host "  ╔══════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "  ║   AgentDash 联合启动（保序模式）     ║" -ForegroundColor Cyan
Write-Host "  ╚══════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host "  root:       $root"
Write-Host "  roots:      $AccessibleRoots"
Write-Host "  backend_id: $BackendId"
Write-Host ""

# ── 健康检查函数 ──
function Wait-ForReady {
    param([int]$Port, [string]$Path = "/api/health", [int]$TimeoutSec = 120)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    $attempt = 0
    while ((Get-Date) -lt $deadline) {
        $attempt++
        try {
            $resp = Invoke-WebRequest -Uri "http://127.0.0.1:${Port}${Path}" `
                -Method GET -TimeoutSec 2 -ErrorAction Stop
            if ($resp.StatusCode -ge 200 -and $resp.StatusCode -lt 400) {
                Write-Host "  [ready] :${Port}${Path} → $($resp.StatusCode)" -ForegroundColor Green
                return $true
            }
        } catch { }
        if ($attempt % 10 -eq 0) {
            Write-Host "  [wait]  :${Port} 第 ${attempt} 次探测..." -ForegroundColor DarkGray
        }
        Start-Sleep -Milliseconds 500
    }
    Write-Host "  [timeout] :${Port} 未就绪（${TimeoutSec}s）" -ForegroundColor Red
    return $false
}

# ── Step 1: 构建 ──
if (-not $SkipBuild) {
    Write-Host "[1/4] 构建二进制..." -ForegroundColor Yellow
    Push-Location $root
    cargo build --bin agentdash-server --bin agentdash-local 2>&1 | Out-Host
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  构建失败" -ForegroundColor Red
        Pop-Location; exit 1
    }
    Pop-Location
    Write-Host "  构建完成" -ForegroundColor Green
} else {
    Write-Host "[1/4] 跳过构建（-SkipBuild）" -ForegroundColor DarkGray
}

$jobs = @()

try {
    # ── Step 2: 启动 server ──
    Write-Host "[2/4] 启动 agentdash-server (:$ServerPort)..." -ForegroundColor Yellow
    $serverJob = Start-Process -FilePath "$root\target\debug\agentdash-server.exe" `
        -WorkingDirectory $root -PassThru -NoNewWindow
    $jobs += $serverJob

    if (-not (Wait-ForReady -Port $ServerPort)) {
        Write-Host "  server 启动失败，退出" -ForegroundColor Red; exit 1
    }

    # ── Step 3: 启动 local ──
    Write-Host "[3/4] 启动 agentdash-local..." -ForegroundColor Yellow
    $localArgs = @(
        "--cloud-url", "ws://127.0.0.1:${ServerPort}/ws/backend",
        "--token", "dev",
        "--accessible-roots", $AccessibleRoots,
        "--name", $BackendName,
        "--backend-id", $BackendId
    )
    if ($NoExecutor) { $localArgs += "--no-executor" }

    $localJob = Start-Process -FilePath "$root\target\debug\agentdash-local.exe" `
        -ArgumentList $localArgs -WorkingDirectory $root `
        -PassThru -NoNewWindow
    $jobs += $localJob

    # 等 local 注册完成（检查 online 端点）
    Start-Sleep -Milliseconds 500
    $localReady = $false
    for ($i = 0; $i -lt 20; $i++) {
        try {
            $resp = Invoke-WebRequest -Uri "http://127.0.0.1:${ServerPort}/api/backends/online" `
                -Method GET -TimeoutSec 2 -ErrorAction Stop
            $data = $resp.Content | ConvertFrom-Json
            if ($data.Count -gt 0) {
                Write-Host "  [ready] local 已注册 (backend_id=$($data[0].backend_id))" -ForegroundColor Green
                $localReady = $true; break
            }
        } catch { }
        Start-Sleep -Milliseconds 500
    }
    if (-not $localReady) {
        Write-Host "  [warn] local 注册未确认，继续启动" -ForegroundColor Yellow
    }

    # ── Step 4: 启动 frontend ──
    Write-Host "[4/4] 启动前端 (:5173)..." -ForegroundColor Yellow
    $frontendJob = Start-Process -FilePath "pnpm" `
        -ArgumentList "--filter", "frontend", "dev" `
        -WorkingDirectory $root -PassThru -NoNewWindow
    $jobs += $frontendJob

    Write-Host ""
    Write-Host "  ╔══════════════════════════════════════╗" -ForegroundColor Green
    Write-Host "  ║       所有服务已就绪                 ║" -ForegroundColor Green
    Write-Host "  ╚══════════════════════════════════════╝" -ForegroundColor Green
    Write-Host "  API:      http://localhost:$ServerPort"
    Write-Host "  Frontend: http://localhost:5173"
    Write-Host "  WS:       ws://localhost:$ServerPort/ws/backend"
    Write-Host ""
    Write-Host "  按 Ctrl+C 停止全部服务" -ForegroundColor DarkGray
    Write-Host ""

    while ($true) {
        $exited = $jobs | Where-Object { $_.HasExited }
        if ($exited) {
            foreach ($p in $exited) {
                Write-Host "  进程 $($p.ProcessName) 已退出 (code=$($p.ExitCode))" -ForegroundColor Yellow
            }
            break
        }
        Start-Sleep -Seconds 1
    }
}
finally {
    Write-Host ""
    Write-Host "  正在停止所有服务..." -ForegroundColor Yellow
    foreach ($j in $jobs) {
        if ($j -and -not $j.HasExited) {
            Stop-Process -Id $j.Id -Force -ErrorAction SilentlyContinue
        }
    }
    Write-Host "  全部已停止" -ForegroundColor Green
}

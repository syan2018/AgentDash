# 发布产物与验收流程 - Implement

## Step 1 - Release Planning Docs

- Expand `design.md` with artifact matrix、version contract、installer/app exe boundary、runner release contract、service validation、cleanup boundary、manual acceptance template、release gate。
- Expand `implement.md` with concrete handoff gates and validation steps。
- Link subtask research files in context manifests。

Validation:

- Trellis `task.py validate` for parent and child tasks。

## Step 2 - Version Consistency Check

- Add or document release version check:
  - root `package.json`。
  - Cargo workspace。
  - Tauri config。
  - runner binary。
  - generated protocol/contracts。
- Define evidence file or release notes section to record results。

Validation:

- version check command once implemented。
- `pnpm run contracts:check`。

## Step 3 - Windows Desktop Release Artifact

- Consume desktop handoff:
  - `pnpm run desktop:bundle`。
  - output glob/path。
  - setup exe name。
  - installed app exe/process name。
  - metadata。
- Write Windows Desktop checklist:
  - install。
  - launch。
  - Desktop API health。
  - Dashboard render。
  - close-to-tray。
  - tray restore。
  - explicit quit。
  - launch at login。
  - start to tray。
  - auto-connect runtime。
  - uninstall cleanup。

Validation:

- `pnpm run desktop:bundle`
- clean Windows manual acceptance。

## Step 4 - Linux Runner Release Artifact

- Consume runner handoff:
  - release build command。
  - binary path。
  - config example。
  - systemd service command。
  - log/status paths。
  - version command。
- Write Linux checklist。

Validation:

- release binary exists。
- `agentdash-local --version`。
- systemd install/start/status/stop/uninstall。
- cloud online/offline/reconnect evidence。

## Step 5 - Windows Runner Release Artifact

- Consume runner handoff:
  - release build command。
  - binary path。
  - Windows Service install command。
  - service name。
  - log/status paths。
  - version command。
- Write Windows Runner checklist。

Validation:

- admin PowerShell service lifecycle。
- cloud online/offline/reconnect evidence。

## Step 6 - Cleanup Validation

- Desktop:
  - installation directory removed。
  - shortcuts removed。
  - uninstall registry entry removed。
  - AgentDash startup entry removed。
  - process exited。
- Linux runner:
  - systemd unit removed/disabled。
  - process exited。
  - config/log/data preserved unless explicit purge。
- Windows runner:
  - service registration removed。
  - process exited。
  - config/log/data preserved unless explicit purge。

Validation:

- platform commands/screenshots recorded in acceptance evidence。

## Step 7 - Manual Acceptance Workbook

- Create final checklist table for:
  - Windows Desktop。
  - Linux Runner。
  - Windows Runner。
- Each row includes:
  - action。
  - expected result。
  - evidence。
  - diagnostics。
  - gate severity。
- Mark unavailable platforms as blocked by environment, not passed。

## Step 8 - Release Gate Dry Run

- Run build/version/contracts gates。
- Execute available platform acceptance。
- Record failures as blocking/warning/info。
- Do not mark release validation done until all blocking gates pass。

## Blockers Before Start

- Windows Desktop task must output real installer path and lifecycle handoff。
- Local Runner task must output Linux/Windows service commands and version command。
- Diagnostics task must output recovery/log paths。

## Risk Checks

- No dev-runtime or target/debug path appears in final user release flow。
- Setup exe is not confused with app exe。
- User workspace/task data is not deleted by uninstall。
- Version evidence matches across artifacts。

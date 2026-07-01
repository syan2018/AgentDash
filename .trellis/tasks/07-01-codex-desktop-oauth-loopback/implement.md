# 修复桌面包 Codex OAuth 本机回调 - Implement

## Ordered Checklist

1. Contracts
   - Add generated DTOs for desktop Codex OAuth prepare/complete/fail requests in `agentdash-contracts::integration::llm_provider`.
   - Register new DTOs in the TypeScript contract generator.
   - Regenerate generated TypeScript and verify `packages/app-web/src/generated/llm-provider-contracts.ts` includes the new shapes.

2. Cloud API flow
   - Split current server-side listener start path from token exchange/persistence.
   - Add prepare endpoints for global provider and user BYOK targets.
   - Add complete endpoint that validates flow ownership, TTL, state, redirect URI, and PKCE challenge before calling OpenAI token exchange.
   - Add fail endpoint for desktop-local callback failures and keep cancel/status endpoints aligned with the same flow store.
   - Keep `exchange_codex_authorization_code`, `extract_codex_account_id`, and `save_codex_credential` as backend-owned behavior.

3. Tauri local OAuth host
   - Add a focused `codex_oauth` module to `agentdash-local-tauri`.
   - Implement PKCE generation and challenge derivation, reusing existing helpers where the dependency boundary is already available.
   - Bind loopback callback listeners for IPv4 and IPv6 on port `1455`.
   - Start cloud prepare only after local bind succeeds.
   - Accept callback, validate route/state/code, call cloud complete, and call cloud fail on timeout/state/callback failures.
   - Implement local cancel so UI cancellation releases listener resources and updates cloud status.

4. Frontend bridge and settings flow
   - Extend `DesktopAppBridge` and Tauri global bridge registration with Codex OAuth start/cancel.
   - Teach `OAuthLoginWizard` or the Codex action factory to use desktop start/cancel when available.
   - In non-desktop environments, render Codex OAuth as unavailable rather than launching a server-side localhost flow.
   - Refresh provider/effective-provider state after completion exactly as the current wizard does.

5. Cleanup
   - Remove or stop using the cloud API path that binds `localhost:1455` from an external server process.
   - Search for remaining calls to the old start endpoints and update them to the desktop-aware path.
   - Confirm no logs/status messages include OAuth secrets or authorization headers.

## Validation Commands

Run focused commands first, then broader checks:

```powershell
cargo test -p agentdash-contracts
cargo test -p agentdash-api llm_providers
cargo test -p agentdash-api oauth_flow
cargo test -p agentdash-local-tauri codex_oauth
pnpm run contracts:check
pnpm run frontend:check
pnpm run desktop:frontend:check
cargo check -p agentdash-local-tauri
```

If test names or crate-level filters differ after implementation, use the closest focused command and record the actual command in the final check notes.

## Required Test Coverage

- Contract generation includes new desktop Codex OAuth DTOs.
- Cloud prepare rejects non-Codex providers and unauthorized global/user credential targets.
- Cloud complete rejects wrong user, unknown flow, expired flow, completed flow, state mismatch, redirect URI mismatch, and verifier/challenge mismatch.
- Cloud complete success saves global provider credential and user BYOK credential with existing preview semantics.
- Tauri listener accepts a valid `GET /auth/callback?code&state` and rejects wrong path, missing code, and state mismatch.
- Tauri start reports port bind failure before browser launch.
- Tauri timeout/fail path marks cloud flow failed.
- Frontend desktop action uses bridge start/cancel when present.
- Frontend non-desktop action does not call server-side localhost start and shows unavailable state.

## Risk Points

- `localhost` may resolve to IPv6 before IPv4; listener tests should cover dual-stack behavior or both bind paths independently.
- Tauri command receives the current access token from the renderer; token values must stay out of logs and error messages.
- The current `OAuthLoginWizard` cancels on unmount; desktop cancel must release local listener and cloud flow without racing a just-completed callback.
- Old API-only Codex OAuth start routes may still be referenced from admin and user BYOK settings paths; search after frontend changes.

## Review Gate Before `task.py start`

- `prd.md`, `design.md`, and `implement.md` have been reviewed.
- `implement.jsonl` and `check.jsonl` contain real spec/research entries, not only seed rows.
- The user has approved entering implementation after reviewing the canonical flow.

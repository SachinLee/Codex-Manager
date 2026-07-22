# Implementation plan

## 1. Prepare and merge

- Confirm the branch and untracked-file boundary.
- Start `git merge --no-commit --no-ff main`.
- Record all conflicts before choosing resolutions.

## 2. Resolve storage and pricing first

- Use main's model catalog v2 storage and model RPC as the baseline.
- Add bridge migrations and v2 pricing/snapshot extensions after migration 116.
- Port cache-write, threshold, Priority, Grok provider-cost, and multiplier
  behavior to v2; add migration and storage regression tests.

## 3. Resolve Gateway behavior

- Retain main's isolated candidate rewrite flow.
- Adapt capability planner/recovery, cooldown UI data, timeout/capacity retry,
  and reasoning-guard observability to the current executor contracts.
- Add focused tests before changing each conflict-heavy behavior.

## 4. Resolve service, desktop, and frontend contracts

- Synchronize Rust RPC, Tauri commands, Web mappings, typed API clients,
  normalizers, TypeScript types, and i18n.
- Recompose main pages around branch-only focused components rather than taking
  whole legacy pages.
- Reconcile Codex Launcher with main startup/tray/window handling.

## 5. Validate and review

- Run conflict-marker and whitespace checks.
- Run focused migration, billing, gateway, RPC, and frontend tests after each
  domain.
- Run full workspace, Web, runtime, build, and desktop build validation.
- Inspect the final diff for legacy runtime resurrection and accidental inclusion
  of the root `package.json`.

## 6. Post-merge remediation (test-first)

- Add Core regressions for hourly-only daily/user/source usage, mixed raw/hourly
  ranges, and paired small/large key filters; then centralize the range source
  and correct binding order.
- Restore Service request-log list-with-summary dispatch, qualify model summary
  SQL, preserve WebSocket cache-write usage, and bound token-activity ranges.
- Repair test fixtures and isolate storage initialization failures without
  changing product behavior to satisfy tests.
- Complete frontend locale coverage and make static source tests CRLF/LF
  independent; reconcile request-log layout and guard-hint contracts.
- Run package checks first, then `cargo test --workspace`, frontend runtime,
  production build, desktop build, diff/marker/legacy scans.

## Key commands

```powershell
git merge --no-commit --no-ff main
cargo test -p codexmanager-core
cargo test -p codexmanager-service
cargo test -p codexmanager-web
cargo test --workspace
pnpm -C apps run test:runtime
pnpm -C apps run build
pnpm -C apps run build:desktop
```

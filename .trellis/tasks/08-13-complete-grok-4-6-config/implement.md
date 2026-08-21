# Implementation Plan

## Ordered Steps

1. Re-read current SQLite row, OMP `models.yml`, model cache schema, and relevant backups; captured before-state showing DB context/max NULL and efforts `low/high`, and OMP cached `grok-4.6` as `reasoning=false`, text-only, 128000/32768.
2. Created timestamped backups for SQLite DB plus WAL/SHM and OMP config/cache files, suffix `grok46-config-20260813-095915`.
3. Updated CodexManager `grok-4.6` in one SQLite transaction: context/max context `200000`, exact efforts `low/medium/high/xhigh`, default `high`, modalities `text/image`; routes/prices/fallbacks untouched.
4. Added OMP `models.yml` provider `modelOverrides.grok-4.6` with reasoning/thinking/input/context/maxTokens/compat target values. OMP loaded the override and rewrote the active v3 provider cache row.
5. The user-level `modelOverrides` explicitly sets `supportsReasoningEffort=true` and `omitReasoningEffort=false`; no npm source patch was needed.
6. `omp models find grok --json` returned `grok-4.6` with contextWindow `200000`, maxTokens `200000`, reasoning `true`, thinking `[low,medium,high,xhigh]`, input `[text,image]`.
7. Read-only regression queries confirmed target child counts and foreign keys; no repository product source was modified.
8. PRD acceptance criteria updated after verification.

## Verification Note

OMP refresh may retain or rewrite its SQLite cache depending on the running session, but the provider-level `models.yml` override is the persistent source and was observed to apply to the active cache and CLI model output. A full interactive picker restart is still required for an already-running OMP process to reload the file.

## Validation Commands / Scenarios

- Python/SQLite read-only query against the live DB and `PRAGMA foreign_key_check`.
- YAML/JSON parse through OMP CLI and model cache inspection.
- `omp models find grok --json`: target fields verified as context 200000, image input, reasoning true, efforts low/medium/high/xhigh, maxTokens 200000.
- Confirmed `grok-4.5` remains 500000 curated metadata in OMP output.
- Live explicit-effort request was not sent; resolved compat metadata was verified instead.

## Rollback Points

- Before SQLite transaction: restore DB/WAL/SHM backup.
- Before OMP cache/config change: restore OMP backup files and remove generated cache only if it belongs to this change.
- If OMP cannot honor a user-level static override, leave the CodexManager DB fix only if independently verified and report the OMP package limitation; do not patch installed source.

## Review Gates

- Do not commit or push local user-directory data.
- After data changes, run Trellis quality check and inspect all changed runtime files/data.


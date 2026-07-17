# Grok 4.5 请求计费实施计划

## Complexity

Large：涉及 gateway usage parsing、pricing resolver、SQLite schema、request-log snapshot、wallet、RPC 和前端日志展示；必须分阶段验证，不能只补一行模型价格。

## Patterns to Mirror

| Category | Source | Pattern |
| --- | --- | --- |
| Official seed lifecycle | `crates/service/src/quota/model_pricing.rs:4`, `:651` | 版本化 official seeds，通过 storage transaction 替换旧 seed，保留 custom rules |
| Usage parsing | `crates/service/src/gateway/observability/http_bridge/aggregate/output_text.rs:133`, `:200`, `:683` | 单一 typed usage、last snapshot merge、共享 JSON/SSE 解析 |
| Cost calculation | `crates/service/src/quota/model_pricing.rs:807`, `:1023` | resolver 选择 effective rule 后在一个公式中生成 typed cost breakdown |
| Request logging | `crates/service/src/gateway/observability/request_log.rs:308`, `:502` | 计价一次，写 request log/token stat，并持久化 pricing snapshot |
| Additive migration | `crates/core/migrations/113_model_price_rules_cache_write_tokens.sql`, `114_request_pricing_snapshots.sql` | nullable/defaulted additive columns + storage ensure helper |
| Storage round-trip | `crates/core/src/storage/request_pricing_snapshots.rs` | insert/list mapper与 ensure table 同步 |
| Frontend contract | `apps/src/types/request-log.ts`, `apps/src/components/modals/model-catalog-modal.tsx` | optional camelCase read model；自定义价格继续复用 model catalog modal |
| Request-log tests | `crates/service/src/gateway/observability/tests/request_log_tests.rs` | 使用内存 storage 写完整日志并断言金额/snapshot |

## Files Expected to Change

具体文件以实施前 `rg` 和 dirty-tree 审计为准；不要覆盖其他 active task 的未提交修改。

| Area | Expected files | Action |
| --- | --- | --- |
| Pricing | `crates/service/src/quota/model_pricing.rs`, `model_pricing_tests.rs` | 增加 xAI seeds、inclusive threshold、reasoning fallback、dual-source result |
| Usage parsing | `crates/service/src/gateway/observability/http_bridge/aggregate/output_text.rs` and focused parser/reader tests | 采集/合并 provider cost ticks/nano-USD |
| Request logging | `crates/service/src/gateway/observability/request_log.rs`, `tests/request_log_tests.rs` | 选择实际/估算费用并写 audit snapshot |
| Retry/wallet | `crates/service/src/auth/app_manager.rs` and relevant Guard event path/tests | 保持 override 与 internal retry 计费一致 |
| Core schema | next available migration, `crates/core/src/storage/mod.rs`, `model_price_rules.rs`, `request_pricing_snapshots.rs` | 持久化 comparator 和 cost-source audit fields |
| RPC | `crates/core/src/rpc/types.rs`, service quota/request-log read mapping and tests | 暴露 optional audit fields |
| Frontend | `apps/src/types/request-log.ts`, pricing-rule types/API normalization, log UI helpers/components, model catalog modal | 显示 cost source/variance，配置 inclusive threshold |

## Ordered Tasks

### Task 1: Dirty-tree and migration allocation gate

- Re-read `git status --short` and diffs for every expected file.
- Preserve unrelated/user changes; stop if overlapping edits cannot be separated safely.
- Allocate the next migration number only after checking current filesystem and index.
- Validate: record the chosen migration number and overlapping-file audit in task notes.

### Task 2: Write failing pricing boundary tests (RED)

- Add tests for all Grok aliases, provider `xai`, Standard/Priority prices and effective-tier selection.
- Seed a pre-existing enabled `aggregate_api_sync` Grok exact rule with `$0` rates and prove the higher-priority official seed wins after upgrade without deleting user data.
- Add `199_999` short and `200_000` long boundary cases.
- Add regression proving GPT-5.6 remains strict `>272K`.
- Add xAI completion+reasoning cases with and without reliable total tokens.
- Add malformed token inputs and unexpected cache-write partial status.
- Validate: focused pricing tests fail for the expected missing behavior.

### Task 3: Add storage migration and round-trip tests (RED -> GREEN)

- Add `long_context_threshold_inclusive` to model price rule storage and RPC payload.
- Add pricing snapshot audit fields from `design.md`.
- Update migration registration, create/ensure SQL, insert/upsert, row mappers and fixtures.
- Test old/legacy rows default correctly and official seed replacement remains transactional.
- Validate: `cargo test -p codexmanager-core model_price_rules` and pricing snapshot/storage tests.

### Task 4: Implement Grok official seeds and local estimator (GREEN)

- Add xAI source and Standard/Priority seeds for canonical model and aliases.
- Implement inclusive threshold selection without changing existing strict rules.
- Extend estimator inputs with total/reasoning and normalize xAI billable output once.
- Keep existing cache partition/clamp behavior.
- Bump official seed version and verify old official rules are disabled while custom rules survive.
- Validate: focused pricing tests pass.

### Task 5: Write failing provider-cost parser tests (RED)

- Cover Chat Completions and Responses JSON.
- Cover final SSE usage, nested `response.usage`, running snapshots and cost-only terminal frames.
- Assert last value wins and invalid/negative values are ignored.
- Cover ticks precedence over nano-USD.
- Validate: focused parser/stream-reader tests fail before implementation.

### Task 6: Implement provider actual cost capture (GREEN)

- Extend typed usage and merge/signal logic.
- Parse cost fields before protocol conversion.
- Ensure stream final usage is available to request logging.
- Avoid exposing or logging sensitive raw response data.
- Validate: parser/stream-reader tests pass.

### Task 7: Implement cost-source selection and snapshot audit

- Always compute local estimate when a Grok rule matches.
- Prefer valid provider cost as base cost; otherwise use local estimate.
- Apply multiplier once and persist raw provider/local/final/variance values.
- Mark tool-bearing token-only fallback partial when tool signal exists.
- Keep legacy `estimated_cost_usd` consumers functional.
- Validate: request-log integration tests for provider actual, fallback, multiplier and long context.

### Task 8: Integrate wallet and internal retry accounting

- No billing-model override: use selected final cost.
- Explicit override: preserve re-rating with complete usage/effective tier.
- Feed provider-selected cost into billable internal retry events when available.
- Prove multiplier and retry charges are not duplicated.
- Validate: focused `app_manager` and reasoning-guard accounting tests.

### Task 9: Extend RPC and frontend

- Add optional camelCase fields for cost source, actual/local cost, variance and threshold comparator.
- Normalize missing values for mixed-version service/desktop compatibility.
- Add request-log badges and diagnostic detail.
- Add inclusive-threshold control to the existing custom pricing form.
- Do not add a Grok-only page.
- Validate: RPC serialization tests, frontend runtime tests and desktop/static build.

### Task 10: Full regression and audit

- Run formatting, relevant focused tests, workspace tests and frontend validation.
- Review `git diff` for hardcoded secrets, accidental history backfill, duplicate formulas and unrelated changes.
- Compare at least one deterministic provider ticks example against local conversion and expected variance.
- Run Trellis full-scope quality check for `codexmanager-core` and `codexmanager-service` specs.

## Validation Commands

Commands may be narrowed during TDD, but the final gate is:

```powershell
cargo fmt --all -- --check
cargo test -p codexmanager-core
cargo test -p codexmanager-service
cargo test --workspace
pnpm -C apps run test:runtime
pnpm -C apps run build
pnpm -C apps run build:desktop
```

If dependency security state is in scope for the final commit gate:

```powershell
cargo audit
pnpm -C apps audit
```

Record exact failures when a command is unavailable or blocked by environment/dependency state.

## Rollback Points

- After Task 4: local Grok seed support is independently testable before selecting provider actual cost.
- After Task 6: provider cost capture is observable but not yet authoritative.
- After Task 7: final cost source switches; if integration findings appear, revert selection to local estimate while retaining nullable audit columns.
- SQLite additions remain additive; do not write destructive down migrations.

## Review Gate

- [ ] `prd.md`、`design.md`、`implement.md` 已经用户审阅。
- [ ] `implement.jsonl`、`check.jsonl` 包含真实 spec/research context。
- [ ] 当前 dirty tree 重叠风险已重新审计。
- [ ] 用户明确批准 implementation。
- [ ] 之后才运行 `task.py start 07-15-grok-4-5-billing`。

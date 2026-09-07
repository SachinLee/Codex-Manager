# Design: GPT-6 Astra model catalog data

## Context and current behavior

The V2 model catalog is seeded from `crates/core/seeds/model_catalog_v2_2026_07_10.json`. `Storage::init()` calls `seed_missing_builtin_models_v2()` on every initialization, so a new fixture entry is inserted into both fresh and already-upgraded databases through the existing idempotent seed path. Prices are represented in integer micro-USD per million tokens, with a base row and zero or more `model_price_tiers` rows.

The official OpenAI model page reports a 1,050,000-token context window, 128,000-token maximum output, and standard prices of $10 input / $1 cached input / $12.50 cache writes / $50 output per million tokens. Requests with more than 272K input tokens use 2x input/cache rates and 1.5x output rates.

## Proposed change boundary

1. Add one builtin `gpt-6-astra` entry to the existing JSON fixture.
2. Advance the fixture revision and update its source metadata.
3. Encode official capabilities needed by the existing model-list contract, including reasoning efforts, text/image input, text output, supported tool flags, and `max_output_tokens=128000`.
4. Encode the base and long-context price tiers. The long tier starts at `272001` because the existing selector chooses the highest tier whose `min_input_tokens <= input_tokens`; this represents “more than 272K”.
5. Extend the existing core storage tests rather than creating a new abstraction or migration path.

除了更新 fixture，还要在用户提供的 `C:\Users\shuan\AppData\Roaming\com.codexmanager.desktop\codexmanager.db` 上执行一次带备份的事务性直接写入；写入前确认没有运行中的 Codex Manager 进程，写入后用只读查询核对模型、价格、tier、路由和 revision。后续应用启动仍由 fixture 的现有幂等 seed 路径保证新安装和其他数据库一致，不新增重复的编号迁移。

## Data contract

| Field | Value |
| --- | --- |
| slug | `gpt-6-astra` |
| display name | `GPT-6 Astra` |
| context / max context | `1050000` / `1050000` |
| max output capability | `128000` tokens |
| base input / cached / cache-write / output | `10000000` / `1000000` / `12500000` / `50000000` |
| long-tier threshold | `272001` input tokens |
| long-tier input / cached / cache-write / output | `20000000` / `2000000` / `25000000` / `75000000` |
| price status | `official` |
| price source | `https://developers.openai.com/api/docs/models/gpt-6-astra` |
| route | account pool `default` → `gpt-6-astra` |

## Data flow and invariants

`fixture JSON → serde BuiltinModelSeed → insert_seed → models/model_prices/model_price_tiers/model_routes → list/get model API → UI and billing`.

- The base price in `model_prices` must match the tier with `min_input_tokens=0`.
- All prices are non-negative integer micro-USD values.
- The builtin route upstream model equals the catalog slug.
- Re-seeding must not overwrite user-edited model rows.
- The exact 272,000-token request remains on the base tier; 272,001 selects the long tier.

## Compatibility and rollback

直接写入前创建同目录备份，事务失败自动回滚；任何结果异常都可通过备份恢复。现有模型和 custom/user-edited rows 保持不变。fixture 更新后，其他数据库在下次正常初始化时获得同一模型。

## Risks and mitigations

- Official docs are protected from static fetches, but the page was loaded in a browser and its visible values were captured; the exact URL is retained in the fixture.
- The fixture contains free-form capabilities JSON. Tests will assert the fields used by the catalog contract and the numeric pricing/context invariants.
- The repository has unrelated uncommitted changes; implementation must touch only the fixture and focused core tests unless validation reveals a required registration change.

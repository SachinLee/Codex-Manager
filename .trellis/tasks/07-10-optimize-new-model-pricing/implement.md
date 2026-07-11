# 优化新模型费用计算实施计划

## Pre-check

1. 确认 task status 已经由用户批准并通过 `task.py start` 进入 `in_progress`；在此之前禁止修改生产代码。
2. Snapshot dirty state，保留用户和其他 active task 的未提交修改。
3. 运行 `py ./.trellis/scripts/get_context.py --mode packages`，加载 `codexmanager-core`、`codexmanager-service` 与 frontend 相关规范。
4. 先阅读现有测试：
   - `crates/service/src/quota/model_pricing_tests.rs`
   - `crates/service/src/gateway/observability/tests/http_bridge_tests.rs`
   - `crates/service/src/http/responses_websocket_tests.rs`
   - `crates/service/src/gateway/observability/tests/request_log_tests.rs`
   - `crates/core/src/storage/tests/request_token_stats_tests.rs`
   - `crates/core/src/rpc/tests/types_tests.rs`
   - apps 中 model catalog、request log 与 runtime normalize 测试

## Phase 1 — RED: Pricing domain tests

1. 先为 `model_pricing` 添加失败测试：
   - `gpt-5.6` alias 与 sol 同价。
   - sol/terra/luna Standard short/long 四分类价格。
   - sol/terra/luna Priority 四分类价格。
   - `272_000` 使用 short，`272_001` 才使用 long。
   - `plain + read + write = total_input`。
   - `read + write > total_input` 时 clamp，无负成本、无双计。
   - old rule 缺 write price 时 fallback input。
   - GPT-5.6 matched custom rule 缺 write price且实际 write > 0 时 `partial`。
   - `gpt-5.6-sol/terra/luna` 不得命中宽泛 `gpt-5` official seed。
   - official model-family boundary 区分 dotted minor family 与日期/version suffix。
   - custom `match_type=prefix` 保持兼容。
2. 为 official seed lifecycle 添加失败测试：
   - 新版本完整 upsert 后旧 official seeds disabled。
   - custom rule 不受影响。
   - 相同 pattern/priority 的匹配结果 deterministic。

## Phase 2 — Schema and storage

1. 新增下一号 migration，添加 generic cache-write price 与 token columns。
2. 扩展 `ModelPriceRule`、`RequestTokenStat`、usage summary/rollup 和 reasoning-guard usage structs。
3. 更新 `model_price_rules.rs` 的 select/insert/upsert/row mapping 与 seed disable transaction。
4. 更新 request token stat insert、query、daily/hourly rollup maintenance 和 aggregate query columns。
5. 添加 storage 回归测试：
   - 新字段 round trip。
   - 旧行 default/NULL 兼容。
   - rollup 正确聚合 cache-write token。
   - historical cost 不被 migration 重算。

Rollback point：此阶段只有 additive schema 与类型扩展；若后续 parser 设计需要变化，可暂停而不启用新 seed。

## Phase 3 — Normalized usage parsing

1. 提取 focused usage normalization helper，避免 output-text parser 与 WebSocket parser 重复字段路径。
2. 扩展 `UpstreamResponseUsage` / `RequestLogUsage`：
   - `cache_write_input_tokens`
   - merge/signal/default 逻辑
3. Responses API 路径解析：
   - `input_tokens_details.cache_write_tokens`
4. Chat Completions 路径解析：
   - `prompt_tokens_details.cache_write_tokens`
5. 更新 SSE、non-stream JSON 与 Responses WebSocket terminal usage。
6. 更新 protocol conversion：
   - Responses → Chat Completions
   - Chat Completions → Responses
   - Anthropic/Gemini adapter 的 total-input normalization 与字段保留
7. 先写失败测试再实现，覆盖：
   - stream/non-stream
   - top-level usage 与 nested response usage 优先级
   - repeated terminal event 不重复累加
   - negative/oversized raw values normalization
   - Anthropic ordinary+read+creation 与 OpenAI total-subset 语义不混淆
   - Gemini 没有明确 write 字段时不臆测 write

## Phase 4 — Unified estimator and official seeds

1. 扩展 `PriceSeed`、`ModelPriceMatch`、`CostEstimate`/`CostBreakdown`。
2. 在 match result 中加入 `matched_pattern`、`price_source`、`match_quality`。
3. 实现 official family boundary matcher、单一 token partition helper 与 `> threshold` 判断。
4. 扩展 rule resolver 和 seed resolver 的 generic/long cache-write price。
5. 添加 GPT-5.6 Standard/Priority seeds 和 alias，bump `PRICE_SEED_VERSION`。
6. 事务化 current seed upsert + old official seed disable，并在成功后 invalidate cache。
7. 确保 Priority seed 不包含推算的 long-context override。
8. 运行 Phase 1 全部测试，确认 RED → GREEN。

Rollback point：如果官方 seed lifecycle 测试不稳定，保留 schema/estimator，暂不 bump seed version。

## Phase 5 — Request log, rollup, reasoning guard and wallet

1. request log 写入：
   - 传入 cache-write token 到 estimator。
   - 写入 `request_token_stats`。
   - 写入 `raw_usage_json.cacheWriteInputTokens`。
2. daily/hourly rollup 与 RPC summary 增加 cache-write token。
3. reasoning-guard retry event 使用完整 usage 与统一 estimator，避免 retry cost 漏算 write。
4. wallet `billing_model_slug` re-rating：
   - 读取 cache-write token 的 camelCase/snake_case/official nested paths。
   - 传递 effective `service_tier`。
   - 复用统一 estimator。
5. 添加回归测试：
   - request log total cost 包含 write component。
   - Priority 请求按 priority seed 计价。
   - billing model re-rating 与直接 model 在相同 usage 下只有价格模型差异。
   - reasoning-guard retry cost 包含 cache writes。
   - missing/partial 状态不会静默变成 `ok`。

## Phase 5.5 — Shadow reconciliation gate

1. 在 wallet 切换前并行计算 legacy/v2 cost，不改变实际 charge。
2. 记录：model、tier、input/read/write/output、context band、matched pattern、match quality、legacy/v2 delta。
3. 使用 `research/input-upstream-reconciliation-2026-07-10.md` 的聚合数据核对：
   - v2 without cache writes = `$41.120590`
   - legacy stored = `$10.8651305`
   - upstream = `$42.208600`
4. 采集完整 cache-write usage 后，确认剩余 `$1.088010` 能由 write component 解释。
5. 若 shadow 对 upstream 的差异持续超过 `3%` 或 `$0.10`，不得切换 wallet。

## Phase 6 — RPC and frontend

1. 扩展 Rust RPC types 与 service upsert validation。
2. 扩展 TypeScript `ModelPriceRuleEntry` / payload、normalizers 与 state drafts。
3. model catalog modal：
   - 基础四分类价格。
   - 折叠 long-context threshold 与四分类价格。
   - 清晰说明空 write price 的 fallback/partial 语义。
4. request log/detail 与 usage summary 增加 optional cache-write token 展示。
5. 添加 frontend tests：
   - 旧 payload 缺字段的 normalize default。
   - 新 payload round trip。
   - validation 阻止负数、非有限数与非法 threshold。
   - Standard/Priority 切换保存各自规则。

## Phase 6.5 — Request-log long-context pricing visibility

1. RED：扩展 `model_pricing_tests.rs`，要求 estimator 返回：
   - `context_band = long` 仅在 Standard 且 input `> threshold` 时成立。
   - input 恰好等于 threshold 为 `short`。
   - GPT-5.6 Priority 为 `single_tier`，不产生 long-context uplift。
   - breakdown 四项之和等于 total，long uplift 等于 applied total 减 short baseline。
   - `partial/missing` 不产生伪精确 uplift。
2. 新增下一号 additive migration，创建 `request_pricing_snapshots` 一对一表及 `context_band`、`price_status` 查询索引；不要修改可能已执行的 migration `113`。
3. 扩展 `ModelPriceMatch` / `CostEstimate`：返回 rule identity、match quality、billing mode、context band、threshold、applied prices、四项费用、short baseline 与 uplift。
4. request log finalize 时，在同一 estimator 结果上写 token stat、总费用与 snapshot；不得为 snapshot 再调用第二套公式。
5. 扩展 storage/RPC：
   - `RequestLogListParams.pricing_band_filter`
   - `RequestLogSummary` 的 pricing snapshot 字段
   - filter summary 的 long count/cost/uplift 与 legacy candidate count
   - list/summary SQL 对 snapshot 使用 `LEFT JOIN`
6. 历史兼容读取：无 snapshot 时只推断 `legacy_candidate/unknown`；保留原费用，不生成 matched rule、breakdown 或 uplift。
7. 前端日志页：
   - 增加独立价格档筛选器。
   - 费用列显示 `长上下文` / `单档` / `历史候选` badge。
   - 详情/tooltip 展示 threshold、matched rule、price status、四项费用和 uplift。
   - 摘要卡展示长上下文请求数、总费用和额外 uplift。
8. 回归测试：storage round trip、旧行兼容、RPC camelCase、Web command mapping、frontend normalize、filter/summary SQL 一致性及 UI badge 文案。

Rollback point：snapshot 表和 RPC 字段均为 additive；如 UI 或查询存在问题，可停止展示/写入 snapshot，不修改历史费用和 wallet。

## Phase 7 — Cleanup

1. 删除 `request_log.rs` 中 dead-code 价格表、旧 resolver 与 `270_000` 阈值。
2. `rg` 确认没有第二份 GPT-5.x 硬编码费用公式。
3. 更新相关设计文档/中文文档，说明 cache write、长上下文边界和 custom rule 字段。

## Phase 6.6 — Automatic compact safety switch

1. RED：为模型目录输出增加测试，关闭时隐藏 `auto_compact_token_limit`，开启时恢复原值，且输入模型对象不被持久化修改。
2. 增加 runtime config：`CODEXMANAGER_AUTO_COMPACT_ENABLED`，默认关闭，并提供 getter/setter。
3. 增加 app setting：`gateway.auto_compact_enabled`，贯通 persisted setting、runtime sync、settings get/patch、TypeScript normalize 和 Zustand 默认值。
4. 设置页增加“自动上下文压缩”开关，说明它控制 Codex 客户端根据模型阈值自动调用 compact；关闭不影响手动 compact。
5. `/v1/models` 返回前应用只读投影策略，不修改 SQLite 中模型目录原阈值。
6. 验证显式 `/v1/responses/compact` 路由和普通 `/v1/responses` 路由没有行为变化。

## Validation Commands

按从窄到宽执行：

```text
cargo test -p codexmanager-service model_pricing
cargo test -p codexmanager-service http_bridge
cargo test -p codexmanager-service responses_websocket
cargo test -p codexmanager-service request_log
cargo test -p codexmanager-core model_price_rules
cargo test -p codexmanager-core request_token_stats
cargo test -p codexmanager-web
pnpm -C apps run test:runtime
pnpm -C apps run build
pnpm -C apps run build:desktop
cargo test --workspace
```

若 package test filter 与当前测试命名不匹配，运行最近的 package-level suite 并记录替代命令，不跳过验证。

## Review Gates

- Gate 1：usage normalization contract 和公式测试先稳定，再修改 wallet 或 UI。
- Gate 2：schema migration 只能 additive；禁止自动重算历史费用或钱包余额。
- Gate 3：official seed 启用前，必须证明 stream/non-stream/WebSocket 都能采集 cache writes。
- Gate 4：wallet 变更按资金边界审查，检查 `partial/missing`、rounding、multiplier 与 idempotent ledger 行为。
- Gate 4.1：shadow reconciliation 通过后才允许 wallet 使用 v2；不得让宽泛 fallback 的 `partial` 结果伪装成 `ok`。
- Gate 5：frontend 字段开放前，backend RPC validation 和 old-payload compatibility 必须通过。
- Gate 6：最终运行 `git diff`，确认没有覆盖其他 active task 或用户未提交修改。

## Deferred Follow-ups

- Anthropic 5m/1h cache-write token 分桶与对应 price fields 的完整 runtime billing。
- Batch/Flex pricing mode。
- Regional processing `10%` uplift，需要可靠 region signal。
- 历史账单补偿/补扣审计流程；本任务不自动执行。

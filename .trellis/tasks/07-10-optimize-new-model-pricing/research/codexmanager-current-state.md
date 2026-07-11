# CodexManager current-state analysis

## 结论

问题不是单一静态价格缺失，而是 cache-write token 从采集到计价、持久化、RPC/UI 全链路未建模。只添加 `gpt-5.6-*` 价格会继续漏算；只添加 token 字段但沿用旧公式则会错算。

## 根因与严重度

### Critical: 新模型缺少 specific 价格规则并静默误匹配旧 `gpt-5` seed

- `PRICE_SEED_VERSION` 仍为 `2026-05-11`：`crates/service/src/quota/model_pricing.rs:4`。
- `PRICE_SEEDS` 从 `gpt-5.5-pro` 开始，没有任何 `gpt-5.6` family：`crates/service/src/quota/model_pricing.rs:47`。
- 仓库中不存在 `gpt-5.6` specific seed。
- resolver 使用 `normalized.starts_with(seed.model_pattern)`：`crates/service/src/quota/model_pricing.rs:705`。
- 因为已有宽泛的 `gpt-5` seed，`gpt-5.6-sol/terra/luna` 会静默按 `gpt-5` 的 `1.25/0.125/10` 价格计算，而不是返回 missing；`price_status` 仍为 `ok`。
- 2026-07-10 `input` 上游实账中，当前本地存储费用为 `$10.8651305`，按 GPT-5.6 官方基础价格重算为 `$41.120590`，上游为 `$42.208600`。

影响：`gpt-5.6`、`gpt-5.6-sol/terra/luna` 的 dashboard、request log、聚合统计和 wallet charge 会被严重低估，同时现有 `ok` 状态无法暴露错误 fallback。该实账窗口少记约 `74.26%`。

### Critical: cache-write usage 未采集

- `UpstreamResponseUsage` 没有 cache-write 字段：`crates/service/src/gateway/observability/http_bridge/aggregate/output_text.rs:14`。
- `RequestLogUsage` 没有 cache-write 字段：`crates/service/src/gateway/observability/request_log.rs:3`。
- 通用 usage parser 只解析 `cached_tokens`，不解析同级 `cache_write_tokens`：`crates/service/src/gateway/observability/http_bridge/aggregate/output_text.rs:195`。
- Responses WebSocket parser 同样只解析 cached input：`crates/service/src/http/responses_websocket.rs:1874`。
- `request_token_stats`、daily/hourly rollups、RPC summary 和前端类型只有 input/cached/output/reasoning。

影响：即使价格表正确，也拿不到 cache-write token 数，费用仍按普通 input 价格估算。

### Critical: runtime price object和公式忽略已预留的 cache-write 价格

- DB `ModelPriceRule` 已有 `cache_write_5m_price_per_1m` / `cache_write_1h_price_per_1m`：`crates/core/src/storage/mod.rs:855`。
- `ModelPriceMatch` 只有 input/cached/output：`crates/service/src/quota/model_pricing.rs:28`。
- `price_from_rule` 完全不读取 cache-write 价格字段：`crates/service/src/quota/model_pricing.rs:615`。
- `estimate_cost_from_price` 仅计算 ordinary input、cached input、output：`crates/service/src/quota/model_pricing.rs:751`。

影响：数据库字段目前是“可存储但不可生效”的死能力。

### High: 长上下文边界条件错误

- 当前 seed/rule 判断使用 `input_tokens >= threshold`：`crates/service/src/quota/model_pricing.rs:630`、`:716`。
- 官方规则为 `>272K`。
- 现有测试把 `272_000` 断言为长上下文：`crates/service/src/quota/model_pricing_tests.rs:180`。

影响：恰好 272,000 input tokens 的请求会提前切到高价档。

### High: 自定义规则 API/UI 无法配置已有高级价格字段

- RPC entry/upsert 仅暴露 input/cached/output：`crates/core/src/rpc/types.rs:1736`、`:1770`。
- service upsert 把 cache-write 与所有 long-context 字段强制写成 `None`：`crates/service/src/quota/read.rs:1603`。
- frontend payload/interface 只有三类价格：`apps/src/lib/api/account-client.ts:176`。
- 模型编辑弹窗只提供三项价格：`apps/src/components/modals/model-catalog-modal.tsx:641`。

影响：用户不能通过产品界面临时修复 GPT-5.6 价格或配置长上下文/cache writes。

### High: model-group wallet 重新计价链路会再次丢失字段

- request success 后写入 wallet raw usage 只包含 input/cached/output/reasoning：`crates/service/src/gateway/observability/request_log.rs:665`。
- billing model 重新估价只提取 input/cached/output：`crates/service/src/auth/app_manager.rs:887`。
- `wallet_charge_for_request` 接收 `service_tier`，但 billing-model 重估没有把 tier 传给 price resolver：`crates/service/src/auth/app_manager.rs:778`、`:921`。

影响：即使 request log 总费用修好，配置了 `billing_model_slug` 的 wallet charge 仍可能漏算 cache writes，且 priority 价格可能退回 standard。

### Medium: 官方 seed 生命周期可能产生旧版本冲突

- seed id 包含日期版本：`official-{PRICE_SEED_VERSION}-{model}`：`crates/service/src/quota/model_pricing.rs:470`。
- 新版本只插入新 id，不禁用旧 official seeds：`crates/service/src/quota/model_pricing.rs:450`。
- rule resolver 对相同 priority/pattern 用 `max_by_key`，旧/新同键时选择依赖列表顺序；storage 排序不含 `seed_version/updated_at`：`crates/service/src/quota/model_pricing.rs:666`、`crates/core/src/storage/model_price_rules.rs:24`。

影响：未来 bump seed version 后，旧价与新价可能同时 enabled，选择结果不具备明确契约。

### Medium: 存在第二份已废弃价格表和不同阈值

- `request_log.rs` 仍保留 `MODEL_PRICE_PER_1K_TOKENS` 和旧估价函数，虽标记 `dead_code`，但其阈值是 `270_000`：`crates/service/src/gateway/observability/request_log.rs:41`、`:126`。
- 实际路径已改用 `quota::model_pricing`：`crates/service/src/gateway/observability/request_log.rs:474`。

影响：维护者容易修改错误的价格源，造成再次漂移。

## 当前手工规则下的漏算示例

若用户只能配置 input/cached/output，1M cache-write tokens 会被当普通 input：

| Model | 当前按 input | 正确 cache write | 少算 | 相对正确值 |
| --- | ---: | ---: | ---: | ---: |
| `gpt-5.6-sol` | $5.00 | $6.25 | $1.25 | 20% |
| `gpt-5.6-terra` | $2.50 | $3.125 | $0.625 | 20% |
| `gpt-5.6-luna` | $1.00 | $1.25 | $0.25 | 20% |

## 现有设计文档的缺口

`docs/zh-CN/report/额度管理中心设计方案.md` 已为 `model_price_rules` 预留 cache-write 字段（`:193`），但第一版成本公式明确只覆盖 input/cached/output（`:348`）。当前实现忠实落地了第一版公式，却没有完成预留字段对应的 usage 与 runtime contract。

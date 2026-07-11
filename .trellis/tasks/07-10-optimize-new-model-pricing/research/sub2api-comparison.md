# sub2api billing comparison

参考仓库：`D:\my-works\sub2api`，只读调查。

## 可借鉴的结构

1. Typed token breakdown：`UsageTokens` 将 ordinary input、cache creation、cache read、5m/1h creation 分开：`backend/internal/service/billing_service.go:139`。
2. Typed cost breakdown：`CostBreakdown` 分别记录 input/output/cache creation/cache read/total/actual：`backend/internal/service/billing_service.go:151`。
3. Cache creation 独立计价，支持 5m/1h 明细和 aggregate fallback：`backend/internal/service/billing_service.go:985`。
4. 长上下文对 cache read 和 cache creation 同步应用 input 侧倍率：`backend/internal/service/billing_service.go:917`。
5. usage log 持久化 cache creation/read token 与分项 cost：`backend/ent/schema/usage_log.go:67`。
6. 定价解析链分离为 channel override → dynamic source → fallback：`backend/internal/service/model_pricing_resolver.go:42`。
7. 有针对 cache read/write、长上下文、service tier 的回归测试。

## 不能照搬的实现与数据

### 新模型价格数据错误

`backend/resources/model-pricing/model_prices_and_context_window.json` 中：

- `gpt-5.6-sol/terra/luna` 三个条目的 input/cached/output 全部相同，实际等于 sol。
- 三个条目都没有 `cache_creation_input_token_cost`，因此 dynamic pricing 下 cache write cost 为 0。
- 这与用户截图和 OpenAI 官方价格表冲突。

### fallback 价格错误

- `gpt-5.5` / `gpt-5.5-pro` 暂回退到 `gpt-5.4`。
- `gpt-5.6-sol/terra/luna` 全部回退到 `gpt-5.4`：`backend/internal/service/billing_service.go:279`。

dynamic source 可覆盖部分错误，但 dynamic 数据恰好也把三种 GPT-5.6 写成同价。

### OpenAI cache-write usage 路径不符合最新官方字段

- OpenAI parser 读取顶层 `cache_creation_input_tokens`：`backend/internal/service/openai_gateway_response_handling.go:747`。
- 最新官方字段位于 `input_tokens_details.cache_write_tokens` / `prompt_tokens_details.cache_write_tokens`。

因此当前 sub2api 的 OpenAI 原生链路可能无法采集 GPT-5.6 cache writes。

### 公式存在 cache-write 双计风险

- sub2api 自己的 protocol conversion 注释确认 OpenAI `input_tokens` 是包含 cache read/cache creation 的 total：`backend/internal/pkg/apicompat/anthropic_to_responses_response.go:98`。
- OpenAI RecordUsage 仅从 input total 中减去 cache read，没有减 cache creation：`backend/internal/service/openai_gateway_usage.go:122`。
- 后续又单独增加 cache creation cost：`backend/internal/service/billing_service.go:965`。

若 GPT-5.6 的 `cache_write_tokens` 是 input detail 子集，直接照搬会把 write tokens 先按普通 input 计一次，再按 cache write 计一次。

## 采用建议

| 能力 | 是否采用 | 方式 |
| --- | --- | --- |
| typed usage breakdown | 是 | 在 CodexManager 建立单一 normalized usage contract |
| typed cost breakdown | 是，分阶段 | 先内部返回分项，后续决定是否持久化全部 cost components |
| cache read/write 分开 | 是 | OpenAI 使用 generic write；Anthropic 继续保留 5m/1h 扩展位 |
| channel/custom override | 是 | 扩展现有 `ModelPriceRule`，不引入第二套 resolver |
| dynamic remote pricing | 否，本任务不做 | 保持官方 seed + 用户 override，避免外部数据错误自动进入 billing |
| sub2api 价格 JSON | 否 | 只作反例与字段参考，价格以 OpenAI 官方页面为准 |
| sub2api OpenAI 公式 | 否 | 使用 `plain = total - read - write`，避免双计 |

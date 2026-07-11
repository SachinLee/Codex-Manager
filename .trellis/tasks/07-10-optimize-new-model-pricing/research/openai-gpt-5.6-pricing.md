# OpenAI GPT-5.6 pricing and usage evidence

调查日期：2026-07-10。

## 官方来源

- Pricing: https://developers.openai.com/api/docs/pricing
- Model guidance: https://developers.openai.com/api/docs/guides/latest-model
- Prompt caching: https://developers.openai.com/api/docs/guides/prompt-caching
- GPT-5.6 Sol model page: https://developers.openai.com/api/docs/models/gpt-5.6-sol

官方站点对普通 HTTP 抓取返回 403，本次通过只读浏览器访问并读取页面正文。

## 已确认事实

1. `gpt-5.6` alias 路由到 `gpt-5.6-sol`。计价种子必须覆盖 alias，不能只覆盖三个带后缀 slug。
2. GPT-5.6 family 开始对 cache writes 单独计费，价格为 uncached input 的 `1.25x`；旧于 GPT-5.6 的模型没有额外 cache-write fee。
3. OpenAI 在 usage token details 中报告：
   - cache reads: `cached_tokens`
   - cache writes: `cache_write_tokens`
   - Responses API: `usage.input_tokens_details.*`
   - Chat Completions API: `usage.prompt_tokens_details.*`
4. 官方示例把 `cached_tokens` 与 `cache_write_tokens` 放在 prompt/input token details 中，因此二者应视为总 input tokens 的分类子集，而不是额外叠加到 input total 的独立 token 总量。
5. 长上下文规则是 input tokens **大于** `272K` 时，对整次请求应用 `2x input` 与 `1.5x output`，不是 `>= 272000`。
6. cache write 属于 input 侧计价：GPT-5.6 的长上下文 cache-write 价格同样是短上下文的 2 倍。

## Standard 价格矩阵（USD / 1M tokens）

| Model | Short input | Short cached | Short cache write | Short output | Long input | Long cached | Long cache write | Long output |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `gpt-5.6` / `gpt-5.6-sol` | 5.00 | 0.50 | 6.25 | 30.00 | 10.00 | 1.00 | 12.50 | 45.00 |
| `gpt-5.6-terra` | 2.50 | 0.25 | 3.125 | 15.00 | 5.00 | 0.50 | 6.25 | 22.50 |
| `gpt-5.6-luna` | 1.00 | 0.10 | 1.25 | 6.00 | 2.00 | 0.20 | 2.50 | 9.00 |

## Priority 价格矩阵（USD / 1M tokens）

官方 Pricing 页切换到 `Priority` 后，只展示单档价格，没有单独的 long-context 列。实现应保存官方明确值，不自行推算 Priority long-context 价格。

| Model | Input | Cached | Cache write | Output |
| --- | ---: | ---: | ---: | ---: |
| `gpt-5.6` / `gpt-5.6-sol` | 10.00 | 1.00 | 12.50 | 60.00 |
| `gpt-5.6-terra` | 5.00 | 0.50 | 6.25 | 30.00 |
| `gpt-5.6-luna` | 2.00 | 0.20 | 2.50 | 12.00 |

## 其他官方计价边界

- Pricing 页说明：符合 data residency 条件、且在 2026-03-05 及之后发布的模型，regional processing endpoint 有 `10%` uplift。
- CodexManager 当前 request usage 没有稳定的 regional processing 计价信号，因此本任务不把该 uplift 混入 GPT-5.6 基础 seed；应作为独立后续能力处理。

## 目标计费语义

设：

- `total_input`：OpenAI usage 的 `input_tokens` / `prompt_tokens`
- `cache_read`：`cached_tokens`
- `cache_write`：`cache_write_tokens`
- `output`：`output_tokens` / `completion_tokens`

分类时必须按剩余空间依次 clamp，避免异常上游字段导致负数或重复计费：

```text
read = clamp(cache_read, 0, total_input)
write = clamp(cache_write, 0, total_input - read)
plain = total_input - read - write

cost =
  plain * input_price
+ read * cached_input_price
+ write * cache_write_price
+ output * output_price
```

所有价格再除以 `1_000_000`。长上下文档位根据原始 `total_input > 272_000` 选择，不能根据 `plain` 选择。

## 兼容降级

- usage 没有 `cache_write_tokens`：按 0 处理，保持旧行为。
- 旧模型出现 cache-write token 但规则没有专用 write 价格：按 input price 处理，符合“GPT-5.6 之前无额外 cache-write fee”的官方口径。
- GPT-5.6+ usage 有 write token 但规则缺专用价格：估算可回退 input price并标记 `partial`；正式 billing 应通过完整官方 seed 或自定义规则避免进入该状态。

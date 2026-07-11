# input 上游费用对账（2026-07-10）

## Scope

- 数据库：`C:\Users\shuan\AppData\Roaming\com.codexmanager.desktop\codexmanager.db`
- 目标时间窗口：北京时间 `2026-07-10 00:00:00` 至 `10:55:00`（end exclusive）
- 实际有记录区间：`08:06:58` 至 `10:53:05`
- 上游 supplier：`input`（aggregate API id `ag_e61927004eee`）
- 请求数：404，其中 399 个 2xx；按上游 usage record 口径纳入所有有 token usage 的请求
- 上游已显示费用：`$42.2086`

本次只读核算没有修改数据库、账单或上游记录。

## Local usage totals

| Model / band | Requests | Input | Cached input | Output |
| --- | ---: | ---: | ---: | ---: |
| `gpt-5.4` short | 11 | 478,154 | 359,552 | 3,838 |
| `gpt-5.6-sol` short | 289 | 27,252,638 | 25,525,760 | 199,314 |
| `gpt-5.6-sol` long (`input > 272K`) | 18 | 5,487,276 | 5,076,736 | 15,631 |
| `gpt-5.6-terra` short | 85 | 6,615,796 | 6,013,184 | 26,972 |
| `gpt-5.4-mini` | 1 | 0 | 0 | 0 |
| **Total** | **404** | **39,833,864** | **36,975,232** | **245,755** |

## Recalculation without cache-write tokens

CodexManager 当前没有持久化 `cache_write_tokens`，因此先令 write 为 0，使用官方新价格逐 model/context band 重算：

| Model / band | Recalculated cost |
| --- | ---: |
| `gpt-5.4` short | `$0.443963` |
| `gpt-5.6-sol` short | `$27.376690` |
| `gpt-5.6-sol` long | `$9.885531` |
| `gpt-5.6-terra` short | `$3.414406` |
| **Total** | **`$41.120590`** |

与上游比较：

```text
upstream                     = $42.208600
recalculated without writes  = $41.120590
difference                   =  $1.088010
difference rate              =  2.577697%
```

该差额符合 GPT-5.6 cache-write premium 的量级。由于不同 model/context band 的 write premium 不同，本地没有 write token 分类时不能从总差额唯一反推出每条请求的 write tokens：

```text
1.25  * sol_short_write_M
+ 2.5 * sol_long_write_M
+ 0.625 * terra_short_write_M
= 1.08801 USD
```

## Current CodexManager stored cost

同一批 `input` 上游记录当前只存储：

```text
$10.8651305
```

相对上游少记：

```text
$31.3434695 / 74.258491%
```

根因不是 GPT-5.6 返回 `missing`，而是当前 official seeds 没有 GPT-5.6 specific pattern，resolver 的 `starts_with` 会让：

```text
gpt-5.6-sol
gpt-5.6-terra
```

静默匹配到宽泛的 `gpt-5` seed，并按 `1.25 / 0.125 / 10` 计价，同时 `price_status` 仍为 `ok`。

## Conclusions

1. 新官方价格和 `>272K` 分档已解释绝大多数差异：从 `$10.86513` 修正到 `$41.12059`。
2. 剩余 `$1.08801`（约 2.58%）需要 `cache_write_tokens` 才能独立精确核验。
3. 优化优先级应为：
   - P0：修复 model matching / 增加 GPT-5.6 specific seeds，禁止新模型静默继承旧 family 价格。
   - P0：采集并计价 cache writes。
   - P1：持久化、rollup、wallet re-rating 与 UI/RPC 全链路一致。
4. 上线后应保留 matched pattern、price source 和 price status，以便未来新增模型时立即发现 fallback。

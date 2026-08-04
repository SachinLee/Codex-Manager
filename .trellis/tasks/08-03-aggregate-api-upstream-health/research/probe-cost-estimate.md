# 主动探测成本估算

## 价格基线

CodexManager 当前模型目录中 `gpt-5.6-sol` 的基础价格为：

- input: 5 USD / 1M tokens
- cached input: 0.5 USD / 1M tokens
- output: 30 USD / 1M tokens

依据：`docs/zh-CN/report/模型目录V2管理与计费说明.md:13`、`crates/service/src/quota/model_pricing_tests.rs:30-33`。小型独立 probe 通常不能依赖 prompt cache，默认按非缓存 input 计算。

## 调度次数

- healthy：15 分钟一次 = 96 次/天 = 2,880 次/30 天。
- degraded：5 分钟一次 = 288 次/天 = 8,640 次/30 天。
- 主动探测按 source opt-in；关闭时没有 scheduled probe 成本，手动“立即检测”仍算一次真实请求。

## 公式

```text
cost_per_probe_usd = (input_tokens * 5 + cached_input_tokens * 0.5 + output_tokens * 30) / 1,000,000
period_cost_usd = cost_per_probe_usd * probe_count
```

## 单上游估算

| 假设 | 单次 | healthy/天 | healthy/月 | degraded/天 | degraded/月 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 20 input + 1 output | $0.00013 | $0.01248 | $0.3744 | $0.03744 | $1.1232 |
| 100 input + 1 output | $0.00053 | $0.05088 | $1.5264 | $0.15264 | $4.5792 |
| 100 input + 16 output | $0.00098 | $0.09408 | $2.8224 | $0.28224 | $8.4672 |

建议预算按 `100 input + 16 output` 档预留，因为 reasoning model/provider 可能不接受 1 token 上限或产生少量 reasoning output；实际实现仍应使用 provider 接受的最低输出上限。

上述成本不包含聚合供应商的按请求费用、最低计费单位、倍率、套餐扣费或货币换算。若供应商每次另收 `$0.001`，healthy/degraded 每月还需分别增加 `$2.88/$8.64` 每个上游。

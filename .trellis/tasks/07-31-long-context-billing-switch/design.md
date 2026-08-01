# 设计：长上下文阶梯计费开关与 GPT-5.6 价格校准

## 边界

- `crates/service` 拥有 App Settings、RPC 和运行时设置读取。
- `crates/core` 拥有模型价格阶梯选择、不可变扣费快照、SQLite 迁移与账本一致性。
- `apps` 仅通过既有 typed App Settings client 显示和保存设置。
- 模型路由保持独立：它选择上游候选，不影响价格选择。

## 设置契约

新增持久化布尔设置 `longContextBillingEnabled`，缺省值为 `true`。服务端在 App Settings 的 get/set 载荷中暴露 camelCase 字段；前端 `AppSettings` 和 normalize 维持同一字段名。

服务端在发起本地计费、日志估算与用量/额度预测时读取该设置。core 不直接读取 service 设置；调用 core 写入扣费快照时，携带本次已解析的 `long_context_billing_enabled`。这避免跨 crate 反向依赖，并使本次选择可审计。

## 阶梯选择

定义共享的“价格选择策略”：

- 开启：选择满足 `min_input_tokens = 0` 或 `min_input_tokens < input_tokens` 的最高档，避免把恰好等于阈值的请求计入长档。
- 关闭：只选择 `min_input_tokens = 0` 的基础档。
- 阈值比较为严格超过关系；272,000 仍属于基础档。

该策略须用于 core 的实际扣费路径和 service 的目录价格解析路径。余额预估没有单笔输入 token，因此继续按基础档预估；它不应伪装为长上下文价格，但须在输出/说明中保持现有语义。

当上游提供 `base_cost_override_microusd` 时，保持现有优先级：不以本地阶梯重算费用。若本地目录存在模型，快照可保留匹配模型元数据；开关不影响上游实报金额。

## 快照与可观测性

每次新请求的 `RequestPricingSnapshot` 复用现有长上下文字段并从 `ChargeSnapshotV2` 填充：

- 开启且命中长档：`context_band = long`，记录命中阈值和短档基线与增量（若可计算）。
- 开启且命中基础档：`context_band = short`。
- 关闭：`context_band = single_tier`，可记录 `price_source`/匹配质量以说明使用基础档。
- 上游实报费用仍标为 `provider_reported`；本地估算仅作比较。

不可变 `request_charge_snapshots` 记录实际选中的 `tier_min_input_tokens` 与单价，历史数据不迁移或回写。开关变更只影响其后的新请求。

## GPT-5.6 迁移

新增编号迁移，将原 121 迁移产生且尚未自定义的内置 GPT-5.6 Sol/Terra/Luna 更新为当前官方价格。候选条件必须限定 `origin = builtin`、`user_edited = 0`、已知官方来源和旧价格/分档，防止覆盖用户编辑值。迁移同步更新基础价格、两个阶梯、价格来源修订和 `builtin_revision`；重复执行保持幂等。

## 兼容性与回滚

- 旧数据库没有该键时默认开启，不改变当前长上下文正确计费行为。
- 关闭开关是无需迁移的即时、向前生效配置变更；重新开启恢复阶梯选择。
- 若需回滚版本，保留已写快照和新价格迁移。只能通过新的前向迁移恢复旧内置价格，不能删除已应用迁移。

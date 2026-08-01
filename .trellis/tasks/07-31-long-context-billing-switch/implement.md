# 实施计划：长上下文阶梯计费开关与 GPT-5.6 价格校准

## 1. 设定持久化与 API

- 在 `crates/service/src/app_settings/` 定义持久化键、默认值和读取/写入 helper。
- 扩展 App Settings patch 解析、当前设置响应和 RPC 测试，验证缺省为 `true`、显式 `false` 可持久化。
- 扩展 `apps/src/types/settings.ts`、`apps/src/lib/api/normalize.ts` 及相应客户端契约。

## 2. 统一本地价格选择

- 在 core 的模型计费路径中引入显式阶梯选择策略或等效输入字段；开关关闭时只选基础档。
- 将服务端的 `model_pricing` 解析函数改为接受同一策略，更新日志估算、API Key 用量和所有配额预测调用方。
- 保持 provider-reported cost 的现有优先级、商业倍率计算和请求幂等性。

## 3. 审计快照

- 将实际命中的阶梯和开关状态映射到 `RequestPricingSnapshot` 的已有上下文字段。
- 为长档计算并存储可用的短档基线/增量；基础档与关闭状态明确区分。
- 验证日志 API 返回和前端现有标签能无歧义显示该结果；仅在必要时补充文案。

## 4. 官方价格迁移

- 新增 GPT-5.6 降价迁移并在 `Storage::init` 注册。
- 更新受影响的 core 迁移 smoke test 与 service pricing tests。
- 迁移只匹配旧的内置官方值，验证自定义价格、`user_edited` 模型和路由不受影响。

## 5. 设置 UI

- 在 `apps/src/app/settings/components/gateway-tab-content.tsx` 的网关计费/策略相邻区域添加 `Switch`。
- 使用既有 `updateSettings` mutation，提供简短可本地化说明：开启按模型长上下文阶梯；关闭按基础档；上游实报费用不变。

## 验证

1. core 单元/存储测试：阈值前、恰好阈值（基础档）、阈值后（长档）；开/关策略；缓存输入；快照幂等；上游实际费用优先。
2. service 单元测试：日志估算、API Key 用量、配额预测均使用同一策略；App Settings 默认值和读写。
3. 数据迁移测试：三个 GPT-5.6 价格正确、用户自定义保护、重复初始化安全。
4. 前端类型/运行时测试和 `pnpm -C apps run build`。
5. Rust 最小相关包测试，目标为 `cargo test -p codexmanager-core -p codexmanager-service`；若变更触及共享工作区基建，再执行 `cargo test --workspace`。

## 风险与检查点

- 任何遗漏的目录价格调用方都会导致显示金额和扣费金额不一致；实施前后用 `rg` 审查所有 `resolve_model_price_from_catalog` 与 `record_charge_snapshot_v2` 调用。
- 不能通过改写旧快照让历史金额“看起来一致”；回归测试必须断言旧记录未改变。
- 工作区已有大量未提交修改及未跟踪迁移；实施仅新增本任务文件和目标模块，不修改或清理其他变更。

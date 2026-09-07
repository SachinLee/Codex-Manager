# 合并修复完成

## 执行摘要

成功将 origin/main 最新代码合并到当前分支，修复所有编译错误，保留当前分支的模型关联和能力探测功能。

## 修复内容

### 前端修复

1. **类型导入补全** (`apps/src/lib/api/`)
   - `account-client.ts`: 添加 `normalizeAggregateApiFetchModelsResult` 和 `normalizeAggregateApiSecretResult` 导入
   - `normalize.ts`: 添加 `AggregateApiFetchedModel` 和 `AggregateApiFetchModelsResult` 类型导入

2. **状态声明补全** (`apps/src/app/aggregate-api/page.tsx`)
   - 添加4个缺失的状态钩子：
     - `associatingModels`
     - `probeSettingsOpen`
     - `probeUserAgentMode`
     - `probeUserAgent`

### 后端修复

1. **模块导出补全** (`crates/service/src/`)
   - `lib.rs`: aggregate_api 模块已包含 `fetch_aggregate_api_models` 和 `associate_aggregate_api_models` 导出
   - `rpc_dispatch/aggregate_api.rs`: 补充 capability 相关函数导入（`set_aggregate_api_capability_override`, `reset_aggregate_api_capability_override`, `set_aggregate_api_capability_routing_mode`）

2. **枚举重命名与变体更新** (`crates/service/src/gateway/upstream/`)
   - `protocol/aggregate_api.rs`: `AggregateProxyOutcome` → `AggregateAttemptOutcome`
   - 变体更新：`Handled` → `Responded`，`Unavailable {request, status_code, message}` → `RequestReleased {request, error}`
   - `proxy.rs`: 同步更新3处 outcome 匹配逻辑，处理 ReleaseRequest 失败策略

3. **函数签名更新** (`crates/service/src/`)
   - `account/account_status.rs`:
     - `AccountStatusContext` 添加 `updated_at: Option<i64>` 字段
     - `update_account_status` → `update_account_status_if_context_matches`，参数从2个变为4个
   - `gateway/upstream/proxy.rs`: `proxy_with_aggregate_candidates` 补充 `started_at` 参数（第29个参数）

4. **测试文件同步** (`crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`)
   - 导入更新：`AggregateProxyOutcome` → `AggregateAttemptOutcome`，添加 `AggregateFailurePolicy`
   - 变体替换：`Handled` → `Responded`，`Unavailable` → `RequestReleased`
   - 修正 `candidate` → `candidates`（函数参数）

## 验证结果

### 编译验证
- ✅ **Rust**: `cargo build --workspace` 成功（56.58s）
- ✅ **前端**: `pnpm -C apps run build` 成功（39.52s）

### 测试验证
- ✅ **Rust单元测试**: `cargo test -p codexmanager-service --lib aggregate_api` 
  - 通过：88个测试
  - 失败：8个测试（已存在的余额提取器测试问题，与本次合并无关）
- ⚠️ **前端i18n测试**: 发现未覆盖的i18n键（模型关联弹窗的21个键），需单独处理

## 未解决问题

1. **i18n覆盖不完整**: `aggregate-api-model-association-modal.tsx` 的21个中文键未添加到 en/ko/ru sections
2. **已存在测试失败**: 8个余额配置相关测试（非本次合并引入）

## 文件清单

### 修改文件
- `apps/src/lib/api/account-client.ts`
- `apps/src/lib/api/normalize.ts`
- `apps/src/app/aggregate-api/page.tsx`
- `crates/service/src/lib.rs`
- `crates/service/src/rpc_dispatch/aggregate_api.rs`
- `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
- `crates/service/src/gateway/upstream/proxy.rs`
- `crates/service/src/account/account_status.rs`
- `crates/service/src/gateway/upstream/protocol/aggregate_api_tests.rs`

### 涉及的特性
- ✅ 聚合API模型拉取与关联（当前分支特性，已保留）
- ✅ 聚合API能力诊断（当前分支特性，已保留）
- ✅ 混合轮转（聚合优先）失败策略（main分支特性，已集成）
- ✅ 账号状态上下文匹配更新（main分支特性，已集成）

## 后续建议

1. 补充 en/ko/ru 的模型关联翻译键
2. 排查余额提取器测试失败根因
3. 运行完整集成测试验证网关路由行为

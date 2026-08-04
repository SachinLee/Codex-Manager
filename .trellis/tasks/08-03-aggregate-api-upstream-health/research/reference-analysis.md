# 聚合 API 上游状态检测参考分析

## 研究基线

- CodexManager: commit `477ce51f2448952e3ae0880d48fe797d8d6ad0c3`
- sub2api: local commit `27e8f69a9e04d5919c7f4b6a4175c34af24e7eb2`
- CLIProxyAPI: `router-for-me/CLIProxyAPI`, commit `ffdb9c9fbc78a6235d59c9ccbdc4243ba35ecdcd`
- CPA-Dashboard: `dongshuyan/CPA-Dashboard`, commit `4d5bf0fb0c395136cc8e03fcd6d3199901e54d60`

## CodexManager 现状

1. 已有主动探测原语
   - `crates/service/src/aggregate_api.rs:2441` 从模型路由中选 probe model。
   - `crates/service/src/aggregate_api.rs:2476` 按 provider 发起真实最小请求，并保存结果。
   - `crates/core/src/storage/aggregate_apis.rs:736` 仅保存最新 `last_test_at/status/error`，没有历史、错误分类、连续成功/失败或下一次调度信息。

2. 已有被动保护原语
   - `crates/service/src/gateway/routing/aggregate_api_cooldown.rs:11` 使用固定 5 次失败阈值。
   - `crates/service/src/gateway/routing/aggregate_api_cooldown.rs:199` 按 `api_id + upstream_model` 记录失败。
   - `crates/service/src/gateway/routing/aggregate_api_cooldown.rs:247` 成功即清除该模型 cooldown。
   - cooldown 只在进程内存中，重启后丢失；错误类别未进入该状态机。

3. 已有后台任务框架
   - `crates/service/src/usage/refresh/runner.rs:348` 提供动态 polling、jitter 和失败退避。
   - `crates/service/src/usage/refresh/batch.rs:115` 已在 polling cycle 中刷新聚合 API 余额。
   - `crates/service/src/usage/refresh/settings.rs:26` 已有统一 background task settings，可扩展独立 health polling 配置。

## sub2api 可借鉴点

1. Scheduled test
   - 每个目标可配置 cron、模型、最大结果数；runner 每分钟扫描 due plans，并限制全局并发为 10。
   - 成功检测可选择 auto-recover，但不是所有计划默认修改路由状态。

2. Channel monitor
   - 状态模型为 `operational/degraded/failed/error`，慢响应可单独标为 degraded。
   - OpenAI、Anthropic、Gemini 通过 adapter 构建请求；用随机 challenge 验证“确实生成了正确响应”，不仅检查 HTTP 2xx。
   - 调度支持 interval + jitter、固定 worker pool、同一 monitor single-flight、请求超时和响应大小限制。
   - 保存明细历史并计算 7/15/30 天可用率。
   - 对自定义 endpoint 做 HTTPS、DNS 和 SSRF 防护；API key 仅在 service 内使用，错误输出经过 sanitize/truncate。

3. 不建议直接照搬
   - CodexManager 的聚合 API 已是受信任管理员配置并复用 gateway upstream client，无需再创建一套独立 endpoint/API-key monitor 实体。
   - 每 15 秒到 60 秒的真实生成请求对付费聚合上游成本过高，默认频率应显著更保守。
   - 30 天全量明细适合 PostgreSQL 监控产品，不适合默认塞入桌面端 SQLite；MVP 应采用有限明细保留或固定条数裁剪。

## CLIProxyAPI / CPA 可借鉴点

1. CLIProxyAPI 请求路径状态机
   - `status.go:4` 将生命周期状态与 unavailable/cooldown 分离。
   - `conductor_cooldown.go:740` 排除 request-scoped error，避免把客户端请求错误归因给凭证/上游。
   - 401、402/403、404、429、408/5xx 使用不同 cooldown 和恢复策略；429 优先采用 Retry-After，否则指数退避。
   - 状态以 auth/model 粒度保存，成功会重置对应模型状态，并把有效 cooldown 独立持久化，重启后仍可恢复。

2. CPA-Dashboard 保守误判策略
   - Codex 通过轻量 Models API 二次确认 token；只有 401 才判定需要重新登录。
   - 网络异常、refresh 失败或临时限流不直接判定凭证失效（`quota_service.py:438`、`app.py:288`）。
   - 批量刷新有可配置并发，避免刷新所有账户时冲击上游。

3. 不建议直接照搬
   - CLIProxyAPI 的主体是 credential pool；CodexManager 的目标是外部 aggregate API source，状态粒度应是 source + protocol/model scope，而不是 auth file。
   - CPA-Dashboard 的 token refresh / quota probing 只覆盖部分 provider，不足以作为通用 upstream availability 判定。

## 结论

推荐采用混合模型：

- 被动观测作为最快、最便宜的故障信号，并按错误类别驱动模型级临时 cooldown。
- 低频主动探测用于无流量目标的可见性，以及 cooldown 到期后的 half-open 恢复确认。
- 聚合 API 级健康摘要从各模型/协议 scope 汇总，不反向覆盖人工 active/disabled。
- 明确错误分类和置信度；确定性故障可快速进入 unavailable，网络/5xx 需连续失败，request-scoped error 不计入。
- 持久化当前状态和有限历史，使 service/desktop 重启后不立刻把故障上游重新加入候选。

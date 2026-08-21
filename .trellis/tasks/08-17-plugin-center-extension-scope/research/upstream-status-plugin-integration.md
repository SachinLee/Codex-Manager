# 上游状态监控与插件接入调研

## 结论

可以用插件做“自定义状态接口适配器”的实验性入口，但当前插件不能直接成为生产级 Aggregate API 健康源：

- Rhai 只有 `http_get(url)` / `http_post(url, body)`，没有自定义 Header 参数，也不能读取已保存 Aggregate API 密钥。
- 插件不能调用 `aggregateApi/health/*` RPC，不能直接写健康状态或事件，也不能注册自己的 UI。
- 插件输出目前只会作为任务输出/日志展示，无法自动进入现有路由冻结和健康状态机。
- 最稳妥的设计是：**插件或配置只描述“如何解析”，service 负责请求、凭据、归一化、持久化和路由语义；前端复用 `/aggregate-api/` 的健康展示。**

## 当前已有的统一能力

`crates/service/src/aggregate_api_health.rs` 已有健康状态机和持久化事件：

- 状态：`unknown`、`healthy`、`degraded`、`unhealthy`、`cooldown`、`recovering`。
- 摘要字段：连续失败次数、阈值、冷却截止、最后观测/探测/成功/失败时间、延迟、HTTP 状态、错误分类、错误原因、观测来源。
- 详情字段：按模型/协议的状态列表和最近事件。
- 健康配置：是否启用、探测间隔、超时、探测模型。
- 连续失败、鉴权失败、限流、模型不支持和高延迟已有分类/冷却语义。
- 健康配置启用时，状态可以参与 Aggregate API 路由阻断；不能只在 UI 侧生成一个红色标签而绕开 service 状态机。

现有 RPC 已提供：

```text
aggregateApi/health/list
aggregateApi/health/get
aggregateApi/health/config/update
aggregateApi/health/probe
aggregateApi/health/reset
```

前端 `accountClient` 已封装对应查询、配置、探测和重置方法，类型也已有统一结果模型。

## 当前 UI 的真实情况

`apps/src/app/aggregate-api/page.tsx` 已每 15 秒查询 `aggregate-api/health/list`，但当前健康摘要主要用于手动测试时选择 `probeModel`。主表格实际展示的是：

- 运行时冷却状态和连续失败；
- 零余额阻断/手动放行；
- 最近连通性测试结果；
- 余额、今日用量、模型路由。

健康摘要中的 `state`、`latencyMs`、`httpStatus`、`errorCategory`、`errorReason` 并未完整呈现在主表格或详情面板。因此这里存在一个现成的 UI seam：在现有“运行状态”列增强，并增加健康详情/事件面板，而不是另造插件专属页面。

`/plugins/` 当前只展示插件任务、权限、手动运行按钮和最近运行日志。它没有插件输出 schema，也没有把任务返回 JSON 归一化为核心业务状态的机制。

## 为什么不能直接用当前插件读取上游状态

### 公开无认证状态接口

可以做一个无权限 Rhai 插件：

1. 调用公开状态 URL；
2. 解析供应商特有 JSON；
3. 返回统一 JSON，例如 `{ ok, state, latencyMs, reason }`；
4. 在插件日志里查看结果。

这适合验证接口结构或低风险内部工具，但结果不会自动进入 Aggregate API 健康状态、路由冷却或现有上游表格。

### 需要认证的状态接口

当前 `http_get`/`http_post` 只接受 URL 和 body；没有 Header、认证引用或 `aggregateApiId` 参数。上游 API 凭据存储在 Aggregate API 记录/secret 体系，不在插件可读的普通 app settings 中。

不应通过以下方式补齐：

- 给插件 `settings:read`，再从设置中寻找密钥；
- 把密钥拼进 URL 查询参数；
- 把密钥写进脚本或市场 JSON；
- 让插件日志返回认证请求或完整响应。

这些方式会破坏凭据隔离和日志脱敏。

## 推荐的深模块接口

建议新增一个 service 内部的统一监控接口，而不是让每个前端页面理解供应商格式：

```text
MonitorDefinition
  target: aggregate_api_id
  request: method + relative_path + body_template
  auth: host_credential_ref
  response_mapping: json paths + status rules
  schedule: interval + timeout

MonitorObservation
  target
  state: healthy | degraded | unhealthy | unknown
  http_status
  latency_ms
  error_category
  reason
  observed_at
  source
  raw_excerpt (optional, bounded, redacted)
```

实现可以有两个 Adapter：

1. **Rust 内置 Adapter**：适用于已知供应商和高价值上游；service 持有凭据，可靠性最高。
2. **受控插件 Adapter**：插件只负责声明/执行解析逻辑，宿主以能力令牌提供绑定的安全响应；插件不能自由读取密钥或写健康状态。

不要让插件直接返回一套“看起来像健康状态”的日志后由前端猜测。统一接口应在 service 侧完成校验和归一化，避免状态显示与路由决策分裂。

## 推荐 UI

默认推荐复用 `/aggregate-api/`：

### 列表行

在现有“运行状态”或“连通性”区域展示：

- 状态徽章：正常 / 降级 / 不健康 / 冷却 / 未知；
- 最近观测时间；
- 延迟；
- HTTP 状态；
- 简短原因；
- 监控来源标识：内置 / 插件 / 手动。

### 详情面板

点击状态后展示：

- 当前统一状态及更新时间；
- 监控适配器/插件名称和版本；
- 最近一次请求结果；
- 解析后的业务字段；
- 最近 N 条健康事件；
- 原始响应截断片段（默认隐藏、限长、脱敏）。

### 插件中心

保留插件管理和原始任务日志，用于：

- 查看脚本版本和权限；
- 修改任务间隔；
- 查看解析失败原文；
- 手动运行调试。

插件中心不应成为判断路由健康的唯一入口。

## 主要风险

- 各供应商的 `HTTP 200` 不一定代表业务健康；需要区分 HTTP、业务字段、响应格式和超时。
- 解析器必须有字段缺失、类型错误、超时、限流、鉴权失败的稳定分类。
- 自定义插件如果可以任意写健康状态，可能错误冻结正常上游或放行异常上游；写入必须由 service 校验。
- 原始响应可能包含密钥、账户标识或内部错误；只能保留限长、脱敏片段。
- 健康探测可能产生计费；应复用现有探测成本统计和间隔/每日上限。

## 证据路径

- `crates/service/src/plugin/runtime.rs:226-323`：插件只注册日志、设置、网络、账号清理函数。
- `crates/service/src/aggregate_api_health.rs:164-527`：统一健康状态、事件、配置、路由阻断判断。
- `crates/service/src/rpc_dispatch/aggregate_api.rs:53-98`：健康 RPC 入口。
- `apps/src/types/api-key.ts:85-157`：前端健康状态/事件类型。
- `apps/src/lib/api/account-client.ts:861-883`：健康查询、配置、探测、重置封装。
- `apps/src/app/aggregate-api/page.tsx:235-245, 847-1177`：健康查询已存在，但主表格主要展示运行时/余额/连通性，健康摘要用于选择探测模型。
- `apps/src/app/plugins/page.tsx:1119-1297`：插件页目前只展示任务和运行日志。
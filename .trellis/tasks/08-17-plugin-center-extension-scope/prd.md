# 插件中心与上游状态监控扩展分析

## Goal

确认部分上游 API 存在不同格式的状态/健康接口时，CodexManager 是否适合通过插件扩展监控；定义统一结果模型、凭据与执行边界，并确定结果在现有界面中的展示位置。

## Current behavior and problem

- `/plugins/` 是 CodexManager 自有的“市场 JSON + Rhai 脚本”自动化插件中心；`/skills/` 的 Codex 插件页则是 Codex CLI Marketplace 的管理封装，两者不是同一运行时。
- Rhai 插件当前只提供 `log`、设置读取、HTTP GET/POST、两种账号清理和任务调度；没有读取 Aggregate API 密钥、写入健康状态、注册 UI 或调用通用 RPC 的宿主桥接。
- Service 已有统一的 Aggregate API 健康状态模型和历史事件：`unknown`、`healthy`、`degraded`、`unhealthy`、`cooldown`、`recovering`，包含延迟、HTTP 状态、失败分类、原因、观测来源和时间。
- `/aggregate-api/` 已每 15 秒查询健康摘要，但当前主要将摘要用于手动测试时选择模型；表格展示的是连通性、运行时冷却和余额状态，没有把健康摘要完整展示出来。
- 插件中心目前只展示任务运行日志，不能自动把插件返回 JSON 变成 Aggregate API 健康状态。

## In scope

- 评估插件适配不同上游状态接口的可行性及限制。
- 设计统一的上游监控结果契约，处理 JSON 结构差异、HTTP 状态、业务状态、延迟、原因和时间。
- 设计安全的凭据使用方式，避免 Rhai 直接获得已保存的上游密钥。
- 复用现有 Aggregate API 健康状态/事件接口和 `/aggregate-api/` 展示能力，给出最小可交付 UI 方案。
- 保留原有插件市场、自研 Rhai 插件能力和安全边界分析。

## Out of scope

- 本轮不直接实现插件运行时、后端适配器或前端页面。
- 不让插件直接读取 API 密钥、直接写 SQLite、直接调用任意 RPC 或自行注册 React UI。
- 不假设不同供应商的状态 JSON 可以在前端临时拼接成统一状态。
- 不实现告警渠道、通知中心、SLA 报表或跨租户监控权限，除非后续明确纳入范围。

## Actors and affected systems

- CodexManager 管理员：通过受控 Config Adapter 定义映射，绑定监控到 Aggregate API，启停监控，查看状态和历史事件。
- codexmanager-service：执行探测、应用受控凭据、归一化结果、持久化健康状态并参与路由冻结。
- `/aggregate-api/`：展示每个上游的统一状态、延迟、最近失败和详情历史。
- `/plugins/`：管理插件安装、任务配置和原始运行日志，不作为统一健康状态的唯一展示面。

## Assumptions and constraints

- 上游状态接口可能是公开 GET，也可能需要 API Key、Bearer、Basic 或自定义 Header；凭据必须由 service 侧持有。
- 监控结果必须区分“接口不可达”“鉴权失败”“业务降级”“响应格式无法解析”和“健康”，不能把 HTTP 200 直接等同于业务健康。
- 现有健康状态可复用，但插件监控不得绕过现有冷却/路由阻断语义。
- 当前结论基于检出源码；本轮不下载或执行第三方脚本。

## Acceptance criteria

### AC-001: 区分两套插件机制

- Scenario: 用户询问“插件中心”能否自研。
- Action: 对比 `/plugins/` 与 `/skills/` 的实现和运行方。
- Expected: 明确前者可通过自定义 JSON 市场发布 Rhai 自动化脚本，后者仅调用 Codex CLI Marketplace。
- Must not: 将 Codex CLI 插件权限误报为 CodexManager Rhai 权限。
- Verification method: 路由、市场和 CLI 封装源码交叉检查。

### AC-002: 列出自研插件可执行能力与边界

- Scenario: 市场条目声明权限并安装后执行任务。
- Action: 检查运行时注册的函数、上下文、调度和持久化逻辑。
- Expected: 完整列出 `log`、设置读取、HTTP、两种账号清理和任务调度；指出未注册通用数据库、文件、进程、前端 UI、路由或 RPC 桥接。
- Must not: 将清单中任意字符串当作实际授予的能力。
- Verification method: `crates/service/src/plugin/runtime.rs` 的函数注册与清单安装路径。

### AC-003: 给出插件安全风险结论

- Scenario: 管理员配置非内置市场并安装带权限的插件。
- Action: 检查权限校验、安装完整性、网络客户端和停用路径。
- Expected: 标记自声明权限、无来源签名/审批、全量设置读取配合网络外传、任意 URL 访问，以及停用插件后手动任务仍可运行的风险。
- Must not: 把仅有 UI 展示当作权限审批或隔离。
- Verification method: 安装、运行时、设置存储与手动运行源码路径。

### AC-004: 兼容不同上游状态结构

- Scenario: 同一批 Aggregate API 的监控接口返回不同 JSON 结构和业务状态字段。
- Action: 通过受控 Config Adapter 定义请求方式、认证来源、JSON 路径和状态映射。
- Expected: service 产出统一观测结果，至少包含健康状态、HTTP 状态、延迟、失败分类、可读原因、观测时间和来源；解析失败可区分于上游明确不健康。
- Must not: 让前端为每个供应商维护一套私有 JSON 解析器。
- Verification method: 适配器契约测试覆盖成功、HTTP 错误、业务错误、字段缺失、格式错误和超时。

### AC-005: 保护上游凭据

- Scenario: 监控接口需要使用已保存的 Aggregate API 凭据。
- Action: 执行监控请求。
- Expected: service 根据 Aggregate API 绑定关系在宿主侧注入凭据；Rhai 插件没有读取凭据、绑定监控或接收原始结果的宿主桥接。
- Must not: 通过 `settings:read`、URL 查询参数或插件日志泄露密钥。
- Verification method: 凭据隔离测试和日志/结果脱敏检查。

### AC-006: 复用现有健康状态与路由语义

- Scenario: 自定义监控连续失败或恢复。
- Action: 写入统一观测结果。
- Expected: 更新现有 Aggregate API 健康状态和事件，并继续遵守既有 degraded/cooldown/recovering 与路由阻断语义；同一 API 不产生互相矛盾的两个健康来源。
- Must not: 只在前端显示红色徽章而不影响/不遵守 service 路由状态。
- Verification method: 状态机、事件持久化和路由冻结回归测试。

### AC-007: 在已有上游界面展示

- Scenario: 管理员打开 `/aggregate-api/` 查看上游列表。
- Action: 查看带自定义监控的 API，并打开详情。
- Expected: 列表展示统一状态、最近观测时间、延迟/HTTP 状态和简短原因；详情展示监控来源、已归一化的状态/HTTP/延迟/失败分类/原因字段和最近事件；插件原始日志仍可在插件中心查看。
- Must not: 把不同供应商原始 JSON 直接倾倒到主表格，或要求用户进入插件日志判断路由健康。
- Verification method: 前端组件行为测试与浏览器实际页面验证。

### AC-008: 明确 MVP 展示范围

- Scenario: 选择监控结果的产品展示面。
- Action: 在规划阶段确认展示位置和历史详情范围。
- Expected: 形成一个明确的 MVP 决策，作为 `design.md` 和 `implement.md` 的输入。
- Must not: 在未决定展示范围前开始实现。
- Verification method: 用户确认规划摘要。

## Key decisions

- MVP 复用现有 `/aggregate-api/` 页面展示统一监控结果：列表行显示状态/最近观测/延迟/HTTP 状态/短原因，详情展示监控来源、已归一化字段和最近事件。
- `/plugins/` 只负责插件安装、权限、任务调度、原始运行日志和调试，不作为路由健康的唯一展示入口。
- 插件不得直接读取 Aggregate API 密钥或写健康状态；service 负责绑定凭据、请求、归一化、持久化和路由语义。
- 统一健康结果应接入现有 Aggregate API 健康状态/事件模型，不能由前端分别解析各供应商原始 JSON。

## Open or blocking decisions

- 2026-08-17：用户选择“仅保留方案”。本任务停留在分析与设计阶段；在获得新的明确批准前，不开始任何生产代码修改。
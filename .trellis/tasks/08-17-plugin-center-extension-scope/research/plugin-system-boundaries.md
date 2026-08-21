# 插件中心与自研插件边界调研

## 结论

可以开发，但 `/plugins/` 不是可加载前端组件或本地二进制的通用扩展平台。它是 **远程市场 JSON 分发 + Rhai 脚本执行 + 手动/定时任务** 的轻量自动化系统。自研插件能由用户配置的市场源分发，首次安装后默认禁用，启用后由本机 `codexmanager-service` 执行。

另有 `/skills/` 中的“Codex 插件市场”：它调用安装在 service 主机上的 Codex CLI 的 `plugin marketplace` / `plugin add` 命令，CodexManager 只负责导入、筛选、展示和安装。其运行与权限模型属于 Codex CLI，不能与 Rhai 插件混用。

## 已确认的自研插件交付方式

1. 托管一个可访问的 JSON 文档；顶层可以是数组或 `{ "items": [] }`。
2. 每个条目至少提供稳定 `id` 和 `scriptBody` 或 `scriptUrl`；任务可指定 `entrypoint`、`manual` 或 `interval`、间隔和默认启用状态。
3. 在 `/plugins/` 选择“自定义源”并保存市场 URL；目录会读取该 JSON，用户可安装、启停、更新、卸载及查看日志。
4. 安装会把脚本、清单、权限和任务写入本地 SQLite；若只给 `scriptUrl`，脚本在安装/更新时下载并保存，而不是每次任务执行时再下载。

最小条目：

```json
{
  "id": "example-health-check",
  "name": "Example health check",
  "version": "1.0.0",
  "scriptBody": "fn run(context) { log(\"running \" + context.plugin.id); #{ ok: true } }",
  "permissions": [],
  "tasks": [
    {
      "id": "run",
      "name": "Run check",
      "entrypoint": "run",
      "scheduleKind": "manual",
      "enabled": true
    }
  ]
}
```

## Rhai 能力矩阵

| 能力 | 要求的清单权限 | 实际宿主函数/行为 | 边界 |
| --- | --- | --- | --- |
| 运行上下文 | 无 | 传入插件、任务、输入和开始时间 | 仅 JSON 数据；未传入账户、数据库连接或 Tauri API。 |
| 日志 | 无 | `log(message)` 写入 service 日志 | 任务结果/错误也持久化并在插件页显示。 |
| 读设置 | `settings:read` | `get_setting(key)`、`list_settings()` | `list_settings()` 直接读取整个持久化设置映射，没有本插件专用的键空间或脱敏层。 |
| HTTP | `network` | `http_get(url)`、`http_post(url, body)` | 请求从 service 主机发出；代码只设置 20 秒超时，未见目标主机 allowlist、私网拦截、请求头或方法扩展。 |
| 账号清理 | `accounts:cleanup` | `cleanup_banned_accounts()`、`cleanup_unavailable_free_accounts()` | 只提供两种预定义删除操作；没有通用账户读写接口。 |
| 计划任务 | 无 | `manual` 或正数间隔的 `interval` | service 启动时启动调度器；每次最多取 100 个到期任务。 |

运行时只注册上表的宿主桥接函数。因此，当前插件**没有官方桥接能力**去读写任意 SQLite 数据、访问文件系统、启动子进程、注册 HTTP/RPC 路由、修改网关协议、调用 Tauri 命令，或注册 React 页面/侧边栏/UI 组件。Rhai 本身可做内存内计算和 JSON 转换，但不是宿主扩展 API。

## 权限与安全边界

### 当前不构成安全授权的地方

- 清单解析接受任意非空权限字符串；安装时原样写入 `permissions_json`。运行时只识别 `settings:read`、`network`、`accounts:cleanup` 三个名称，未知权限不会产生能力。
- 插件页显示权限，但安装路径没有单独的“同意这些权限”状态或权限授予表。安装或更新一旦带入这三个名字，运行时就注册相应函数。
- 自定义市场和 `scriptUrl` 使用普通 HTTP 客户端下载；检索范围内没有插件签名、哈希校验、可信发布者或来源 allowlist。
- `settings:read` 与 `network` 联用时，脚本可以读取完整持久化设置映射，并把值 POST 到任意 service 主机可达的 URL。设置键中包括环境覆盖配置与 Web 访问密码哈希，因此这不是低风险组合。
- `network` 同样能访问 service 主机可访问的私网/回环地址，形成 SSRF 风险。
- Rhai 限制显式设置为 50,000 次操作；HTTP 调用允许阻塞到 20 秒。未看到网络目的地策略或插件级资源配额。

### 启停语义的缺口

- 定时任务查询要求任务启用、插件状态 `enabled`，并排除 `manual`，所以停用会阻止定时执行。
- 但手动运行路径只在“插件已停用且任务不是 `manual`”时拒绝；前端对已安装任务始终展示“运行”按钮。因此已停用插件的手动任务仍可被执行。不要把当前“停用”当作强隔离或紧急熔断。

### 更新行为

更新会覆写本地脚本、权限和 manifest，并在同一事务中删除后重建该插件的任务定义；插件启停状态保留。更新前没有以签名或用户确认来锁定权限升级，因此市场维护者需要把更新视为代码发布。

## Codex 原生插件入口

`/skills/` 的“Codex 插件”要求 Codex CLI 报告的插件为本地 marketplace 条目，并要求插件目录有 `.codex-plugin/plugin.json`，其中包含名称、版本和安全相对路径的 `skills`，且该目录下至少存在一个标准 `SKILL.md`。安装由 CodexManager 重新构建清单后调用：

```text
codex plugin marketplace add --json [--ref <ref>] -- <owner/repo>
codex plugin add --json -- <plugin-id>
```

这条通道适合发布 Codex Skills/插件包，但其能力、沙箱、工具和会话注入由使用该包的 Codex CLI 决定；CodexManager 没有为其增加 Rhai 的 `network`、`settings:read` 或账号清理权限。

## 建议的发布与治理方式

1. 先把自研需求压在无权限或仅 `network` 的脚本内；不要同时授予 `settings:read` 与 `network`。
2. 用受控 HTTPS 私有市场发布，固定版本并对 script URL 和市场文档做独立签名/哈希治理；当前产品不替你验证。
3. 每个插件只做单一自动化动作；关键删除操作必须只发布在内部市场，并以手动任务开始。
4. 需要 UI、细粒度资源、账户操作、数据库、回调或可撤销权限时，不应继续扩展 Rhai 清单；应单独设计受能力令牌约束的插件 SDK 和授权模型。

## 证据

- `apps/src/app/plugins/page.tsx:70-81, 758-850, 1120-1229`：插件中心仅支持内置/自定义市场，展示任务、权限、手动运行和定时间隔。
- `crates/service/src/plugin/catalog.rs:143-312, 509-635, 763-864`：远程 JSON 获取与解析，安装/更新时下载脚本、持久化权限，首次状态为 disabled。
- `crates/service/src/plugin/runtime.rs:20-27, 119-180, 226-360`：任务运行、50,000 操作上限、已注册宿主函数与 HTTP 行为。
- `crates/core/src/storage/plugins.rs:98-120, 332-406`：到期任务仅运行 enabled 插件，更新替换任务。
- `crates/service/src/app_settings/store.rs:34-40` 与 `crates/service/src/app_settings/shared.rs:45-49`：全量设置读取及潜在敏感键。
- `crates/service/src/codex_skills_marketplace.rs:124-152, 204-267, 844-951, 954-1002`：Codex CLI Marketplace 的导入、安装和兼容插件筛选。

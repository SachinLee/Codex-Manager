# CodexManager 与 sub2api 本地网关比较

## 问题与判定基准

使用者在 Windows 11 本机以 CodexManager 管理多个上游 API，要求上游故障时自动继续尝试下一个候选；优先级是该能力不倒退、日常维护简单、常驻资源尽量小。未修改任一项目，也没有启动网关或创建账号。

## 已核实事实

### CodexManager

- `README.md:163-178` 将“聚合 API”定义为管理第三方最小转发上游，并说明本地网关供 Codex CLI、Gemini CLI、Claude Code 和其他 OpenAI 兼容工具使用。
- `crates/service/src/gateway/README.md:68-92` 明确把显式 route、cooldown、failover、候选管理、超时、重试和退避列为网关职责。
- `crates/service/src/gateway/upstream/protocol/aggregate_api.rs:1623-1768` 会跳过冷却、零余额和日预算不合格的聚合 API 候选；`:1889-1919` 建立有序候选列表并逐个尝试。
- `aggregate_api_tests.rs:1250` 的 `chat_nonstream_malformed_response_fails_over_to_later_candidate` 已执行通过：`cargo test -p codexmanager-service chat_nonstream_malformed_response_fails_over_to_later_candidate --lib`，耗时 2.36 秒。它覆盖首个非流式候选返回畸形响应后向后续候选切换。
- 另一个相近的空流预检用例在尝试两个候选并确认最终 200 后，因首个候选健康状态未写入而失败：`chat_preflight_empty_stream_fails_over_to_later_candidate`，失败位置 `aggregate_api_tests.rs:1219`。因此“切到后续候选”在该用例中发生，但不能声称空流失败后的健康冷却记录当前完全正确。
- 持久化为本地 SQLite：`README.md:204-210` 指出桌面数据文件为 `codexmanager.db`；`crates/service/Cargo.toml:12` 使用 bundled `rusqlite`。
- Windows 桌面端操作路径是启动程序后点击“启动服务”（`docs/zh-CN/report/运行与部署指南.md:9-13`）。服务版可单独启动 `codexmanager-service`，或由 `codexmanager-start` / `codexmanager-web` 拉起 service + Web（同文件 `:83-100`）。
- 现有 `target/release/CodexManager-0.5.3.4.exe` 为 50,268,160 B（约 47.9 MiB）。该文件大小不是内存占用。

### sub2api

- `README_CN.md:182-215` 定位为订阅配额分发和管理平台，覆盖账号、API Key、精确计费、粘性会话调度、并发/限速和支付；技术栈是 Go + Vue + PostgreSQL + Redis。
- 官方 Linux 安装要求 PostgreSQL 15+、Redis 7+ 和 root 权限（`README_CN.md:230-267`）。
- 标准和本地目录 Compose 均启动 `sub2api`、PostgreSQL、Redis 三个服务；应用显式依赖后两者健康检查（`deploy/docker-compose.yml:14-81,178-285`；`deploy/docker-compose.local.yml:22-84,181-269`）。PostgreSQL 默认 `shared_buffers=128MB`（`deploy/docker-compose.yml:203-221`），Redis 开启 AOF 每秒落盘（`:249-285`）。
- `RUN_MODE=simple` 仅隐藏 SaaS 功能和跳过计费（`README_CN.md:734-740`）；同一 `docker-compose.local.yml` 仍要求并拉起 PostgreSQL 与 Redis。因此它不是单二进制/SQLite 的本机轻量模式。
- 账户故障切换为有界重试：配置 `max_account_switches`（`backend/internal/config/config.go:997-1009`），网关默认普通平台 10 次、Gemini 3 次（`backend/internal/handler/gateway_handler.go:80-113`），并将失败账号从后续选择中排除（`:611-668`）。
- `go test ./internal/handler -run "^TestResponsesCredentialFailoverLoop$" -count=1` 已通过，耗时 349.13 秒；这是账号凭据失败时选择健康账号的单元覆盖。

## 本机观察

- 检查时没有运行中的 `CodexManager.exe`、`codexmanager-service.exe`、`sub2api.exe` 或 `com.docker.backend.exe`。因此没有可用于比较 RSS/CPU 的实际常驻进程；不能从静态文件或源码推算精确内存数字。
- Docker CLI 已安装（29.1.3），但 Docker Desktop Linux Engine 未运行；执行 `docker version --format "{{.Server.Version}}"` 无法连接 named pipe。若选择 sub2api 的官方 Compose 路径，需要先启动 Docker Desktop，Windows 上还会引入其虚拟化运行时开销。
- `D:/my-works/sub2api` 当前没有可供测量的 `sub2api(.exe)` 构建产物。

## 结论

**保留 CodexManager 作为本机主网关。** 对现有“多个第三方上游 API + 自动降级”用途，它的聚合 API 候选路由与数据模型直接匹配，且使用本地 SQLite；不需要为了切换能力再引入 PostgreSQL、Redis 和 Docker Desktop。

sub2api 的账号池切换能力真实存在，但它更适合多用户、配额分发、计费、并发与支付管理。其功能广度不能抵消本机部署的三服务依赖和迁移成本；当前未验证现有 CodexManager 聚合 API 配置可一对一迁移到它的账号/分组模型。

## 推荐运行形态

1. 以 CodexManager 桌面端维护聚合 API、路由、模型映射与账户信息。
2. 如不需要常驻 GUI，使用 Release 的 `codexmanager-service` 仅承载网关；需要管理界面时再启动 Web/桌面端。服务版的官方启动方式见 `运行与部署指南.md:83-100`。
3. 保留至少两个同模型可用的聚合 API 候选，并为每个 route 设定确定顺序；实际切换路径受候选冷却、余额封锁、日预算和能力兼容性过滤影响。
4. 将“空流预检失败后健康状态未写入”的测试失败视为已知风险：升级或改动该模块前，先复跑上述两个聚合路由测试；在使用中若发现同一空流上游被反复优先尝试，应检查请求 trace 和冷却状态。

## 未测量项

- 没有对任一网关做真实 API 请求、并发压测或持续 RSS/CPU 采样，避免触发实际账号流量与改变本机运行状态。
- 因此本文只比较架构下界、服务数量和可观察依赖；不提供未经测量的 MB 级内存结论。

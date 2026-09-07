# 技术设计：恢复 OMP 请求日志会话身份并隔离标题刷新

## 1. 质量画像与设计目标

- **质量画像：`standard`**。本任务修复普通网关日志行为和本机只读标题投影；不涉及认证/授权、资金、secret、数据库 schema/migration、远程文件访问或破坏性操作。
- 设计对应 `prd.md` 的 AC-001 至 AC-004：只在有明确 OMP 身份证据时把 `x-client-request-id` 用作最后一级日志身份候选；`requestlog/sessionTitles` 永远从已发布快照返回，冷/过期文件扫描只在后台单飞执行。
- 保持一个深模块：`crates/service/src/requestlog/requestlog_session_titles.rs` 继续通过 `list_request_log_session_titles(limit)` 暴露唯一标题 resolver 接口，并在内部拥有发现、增量文件缓存、合并、快照发布、刷新调度和降级。RPC/Tauri/Web/UI 不新增第二套缓存或调度器。

## 2. 已核实的当前行为与证据

### 2.1 会话身份链

1. `IncomingHeaderSnapshot` 已在 axum 与 tiny-http 两条解析路径捕获 `x-client-request-id`，并提供 `client_request_id()`；同时捕获 `User-Agent`、`originator`、三个 session header alias 与 `x-codex-parent-thread-id`（`crates/service/src/gateway/request/incoming_headers.rs:7-30,44-149,210-315,492-538`）。
2. `resolve_request_log_session_id` 当前只按 `session_id > parent_thread_id > metadata candidates` 返回，不读取 `client_request_id`（`crates/service/src/gateway/local_validation/request.rs:2394-2401,2447-2462`）。Aggregate 和普通路径都调用该 helper（同文件 `:2004-2009,2394-2401`），所以最小修复点在这个私有 resolver，而不是各 finalizer。
3. 请求体的显式 session/thread 字段与受限 `prompt_cache_key` 已由 `request_helpers.rs:138-234` 统一校验；任意 cache key 和 `pck:v1:` route anchor 已被拒绝，不应为本任务另建一套 body 解析。
4. 当前本机 OMP `18.0.4` 的已安装源码使用 `Bun.randomUUIDv7()` 生成会话 ID（`C:/Users/shuan/.bun/install/global/node_modules/@oh-my-pi/pi-coding-agent/src/session/session-manager.ts:93-99`）；Codex Responses provider 同时发送 `originator: pi`、`User-Agent: omp/<version>`、`conversation_id = sessionId`、`session_id = sessionId` 与 `x-client-request-id = sessionId`（`.../@oh-my-pi/pi-catalog/src/wire/codex.ts:12-41`、`.../@oh-my-pi/pi-ai/src/providers/openai-codex-responses.ts:4297-4352`）。先前任务的 OMP `17.1.8` 研究也记录了 session ID 与 `x-client-request-id` 同值（`07-30-omp-session-title/research/session-title-feasibility.md:8-12`）。
5. 活动 PRD 的实机数据库证据表明新 `/v1/responses` 行没有 `session_id`/`conversation_anchor`。因此本设计只补齐日志 metadata 的最后一级 fallback，不改写 `IncomingHeaderSnapshot.session_id`，也不把该值送入 routing/session-affinity。

### 2.2 标题读取链

1. `list_request_log_session_titles` 当前在 RPC 调用线程中同步读取 Codex session DB，然后依次调用 Pi/OMP cached resolver（`requestlog_session_titles.rs:104-143`）。
2. Pi/OMP cache 命中仅在五秒内返回；冷或过期时会在调用线程中完成 root 打开、目录发现、metadata 检查、文件解析和 cache 替换（同文件 `:270-344`）。扫描期间虽未持有 cache mutex，但 RPC caller 本身仍等待。
3. UI 每五秒调用一次标题 RPC，同时独立轮询 `requestlog/list_with_summary`（`apps/src/app/logs/page.tsx:160-200`）。RPC adapter 直接同步调用 resolver，权限仍为 admin-only（`crates/service/src/rpc_dispatch/requestlog.rs:107-116`）。
4. Tauri command 只是 `spawn_blocking` 后调用 TCP RPC（`apps/src-tauri/src/commands/shared.rs:14-24`、`commands/requestlog.rs:39-70`）；默认 socket connect timeout 为 400ms、read/write timeout 为 10s，标题方法没有特例（`apps/src-tauri/src/rpc_client/transport.rs:9-16,29-74,90-147`）。提高该 timeout 只会延迟失败，不能修复 service RPC 饥饿。
5. 当前 resolver 已包含 OMP/Pi main/child 的有界发现、parent projection、路径+mtime+size 增量缓存、Codex 冲突优先级和 `parentSessionId`/`parentTitle`（`requestlog_session_titles.rs:27-102,145-389,465-714`）；前端 normalizer 和日志页继续以 child `sessionId` 建 Map（`apps/src/lib/api/normalize.ts:1820-1847`、`apps/src/app/logs/page.tsx:276-280`）。本任务不得重做这些已存在行为。
6. 实机 main session `01a03264-f861-7588-9c02-ef3244d5e5c6` 已在标题 RPC 中以 `source=omp` 返回且有标题，证明有效日志 `session_id` 可直接命中现有 read model；本任务不需要新增 OMP 扫描布局。
7. `01a03276-2dbb-7251-aa41-318f20ef4acd` 当前解析为 `source=codex` 且无标题，不能作为“遗漏 OMP child”的证据；不得据此放宽深度、递归或 parent 推断。

## 3. 变更边界

### 3.1 范围内

- 在 `resolve_request_log_session_id` 的现有优先级末尾增加一个严格 OMP-only `x-client-request-id` fallback。
- 在现有标题 resolver 模块内增加**合并结果快照**和后台刷新单飞状态；Codex、Pi、OMP 的所有可能慢读取都进入 worker。
- 让顶层刷新能区分“合法空 source / 局部坏条目”和“source 级扫描错误”，从而只在完整刷新成功时原子发布新快照。
- 增加 gateway、resolver 并发/失败、RPC、现有前端展示以及桌面包实机回归证据。

### 3.2 明确不变

- `request_logs` schema、migration、历史行与 `RequestLogTraceContext` 字段不变。
- 请求 routing、Aggregate candidate 选择、账号路由、session-affinity、上游 body/header 和 HTTP/WebSocket/Aggregate finalizer 语义不变。
- OMP/Pi 协议、会话文件和目录布局不变；不新增私有 header。
- `requestlog/sessionTitles({limit?})` 方法名、admin-only 权限、返回 item shape、Tauri/Web command 映射、TypeScript normalizer 和日志页 Map key 不变。
- OMP/Pi scanner 的固定布局、深度、文件/目录/字节预算、no-follow/reparse-point 防护和 transcript/private prompt 读取边界不变。
- Tauri 的 400ms connect timeout 与 10s default I/O timeout不变；本任务不能以延长 timeout 掩盖 service 同步扫描。

## 4. OMP 身份 fallback 的精确规则

### 4.1 解析优先级

最终日志身份优先级固定为：

```text
session-id / session_id / x-session-id
  > x-codex-parent-thread-id
  > 现有显式 body metadata / 受限 prompt_cache_key candidate
  > 严格 OMP x-client-request-id fallback
  > None
```

这保证现有 header/body 合约不变。fallback 只返回给 `LocalValidationResult.request_log_session_id`；不得写回 `IncomingHeaderSnapshot`，不得进入 conversation anchor、routing 或 affinity。

### 4.2 严格 OMP predicate

只有以下条件**全部**成立才接受 `x-client-request-id`：

1. 现有更高优先级 header/body candidate 全部缺失。
2. 规范化后的客户端路径精确为 `/v1/responses`；不扩展到其他 endpoint。
3. `originator` trim 后精确等于 `pi`。
4. `User-Agent` trim 后匹配 `omp/<non-empty-version>`；不接受只含 `omp`、任意包含子串或 `codex_cli_rs`。
5. `x-client-request-id` 是 canonical lowercase RFC-variant UUIDv7：36 字节、固定连字符位置、其余仅 `[0-9a-f]`、version nibble 为 `7`、variant nibble 为 `8|9|a|b`。

条件 2–5 来自当前 OMP 18.0.4 的真实 provider/session-manager 合约，而不是把“所有 Codex-like request”猜成 OMP。该规则会有意拒绝普通客户端随机 request ID、非 UUID request ID、UUIDv4、缺任一 OMP marker 或其他 endpoint；拒绝时继续持久化 `NULL`，请求照常转发。

实现时复用现有 UUID-shaped helper 能复用的结构校验，再叠加 OMP 专属 version/variant/canonical 检查；不要改变现有 body `prompt_cache_key` 可接受集合。

## 5. 标题快照深模块

### 5.1 外部接口

保留：

```rust
pub(crate) fn list_request_log_session_titles(
    limit: Option<i64>,
) -> Result<Vec<RequestLogSessionTitle>, String>
```

接口的新性能不变量：调用只允许做 limit normalization、短 mutex 临界区、`Arc` clone、至多一次 worker spawn 尝试和已发布 slice 的 `Vec` clone；**不得在 caller 上打开 DB、枚举目录或读取文件**。返回值始终是某一完整发布代的 prefix，而不是刷新中的半成品。

### 5.2 状态

在现有 resolver 模块中增加一个进程级 `SessionTitleSnapshotCache`，内部状态最少包含：

```text
published: Arc<[RequestLogSessionTitle]>  // 最近一次成功发布；初始为空
published_generation: u64                // 0 表示尚无成功刷新
refresh_in_flight: bool                  // 全局 Codex+Pi+OMP 单飞
next_refresh_at: Option<Instant>          // 从最近一次刷新完成/启动失败后计时
```

完整快照按 `MAX_SESSION_TITLE_LIMIT = 2_000` 构建并排序；每个 caller 只返回其 normalized limit 的 prefix。使用 `Arc` 是为了在锁内只复制指针，字符串/Vec clone 在释放锁后进行；现有返回类型决定每次 RPC 最终仍需拥有一个 `Vec`，不新增 wire 类型。

现有 OMP/Pi path+mtime+size cache 继续作为同一深模块内的增量扫描实现；它不再决定 RPC caller 是否等待。不要在 UI、RPC adapter 或每个 source 外再增加快照。

### 5.3 调用与状态转换

| 调用时状态 | caller 行为 | 调度行为 |
| --- | --- | --- |
| generation=0、无 worker | 立即返回空 snapshot | 标记 in-flight 后启动一个 worker |
| generation=0、worker 运行中 | 立即返回空 snapshot | 不重复启动 |
| 已发布且未到 `next_refresh_at` | 立即返回最近 snapshot | 不启动 |
| 已发布且已过期、无 worker | 立即返回旧 snapshot | 标记 in-flight 后启动一个 worker |
| 已发布且 worker 运行中 | 立即返回旧 snapshot | 不重复启动 |

调度必须先在 mutex 内把 `refresh_in_flight=true`，再释放锁并 spawn，防止并发 caller 同时启动。扫描和合并全程在锁外。worker 完成后只在一次短临界区中提交结果：

- **成功**：原子替换 `published`，`generation += 1`，清除 in-flight，设置 `next_refresh_at = completion + 5s`。
- **刷新错误**：不修改 `published`/generation，清除 in-flight，设置下一次尝试为 `completion + 5s`。
- **worker spawn 失败**：本次调用仍返回当前 snapshot；立即清除 in-flight，并从失败时刻延后五秒再尝试，避免每个轮询形成 spawn storm。

五秒按完成时刻计算，慢刷新完成后不会立刻再开下一轮。任何时刻至多一个合并刷新；RPC caller 不 join worker。

### 5.4 刷新成功与错误分类

后台 builder 顺序读取 Codex、Pi、OMP 并只在整体 `Ok` 时 merge/publish：

- sessions root **不存在**：该 source 合法为空，刷新可成功；删除 root 可在下一次成功刷新移除旧标题。
- root 存在但无法安全打开/枚举、Codex DB 顶层读取失败：source 级刷新错误，整代不发布，保留上一完整 snapshot。
- 单个 project/derived 目录不可读、坏 JSONL、无标题、非法 ID、symlink/reparse point、单文件 metadata 竞态：沿用现有局部跳过；不读取更多内容补救。
- 达到既有 file/directory/byte budget：这是有界的成功结果，发布此前已验证候选；不得放宽预算或递归。
- 初次刷新错误且无历史成功代：返回空 snapshot，五秒后可重试；RPC 和日志列表仍正常。

为了实现该区分，顶层 root open/list helper 应返回内部 `Result`，而 direct fixture helper 可继续把缺失 root 投影为空。不能再用同一个 `Option/Vec::new()` 同时表达“root 不存在”和“权限/枚举失败”，否则会把有效旧快照错误清空。

## 6. 数据流

```text
OMP /v1/responses headers
  -> IncomingHeaderSnapshot（既有捕获，不改写）
  -> resolve_request_log_session_id
       standard header > parent > body > strict OMP client-request fallback
  -> LocalValidationResult.request_log_session_id
  -> 既有 HTTP / WS / Aggregate RequestLogTraceContext
  -> request_logs.session_id (child 仍是 child ID)

Codex DB + bounded OMP/Pi metadata scanners
  -> existing incremental per-file caches
  -> background full merge (single flight)
  -> atomically published RequestLogSessionTitle snapshot
  -> list_request_log_session_titles(limit) immediate prefix
  -> existing admin RPC / Tauri or Web / normalizer
  -> child sessionId keyed Map + existing parentTitle projection
  -> SessionInfoCell / title search
```

父关系仍只来自标题 read model 的已验证文件布局；gateway fallback 不创建、覆盖或猜测 `parentSessionId`。

## 7. 失败模式、兼容性与安全

| 失败/输入 | 结果 |
| --- | --- |
| 任意普通 `x-client-request-id` | predicate 不成立，日志 session 仍为空/使用更高优先级 candidate |
| OMP markers 正确但 UUID 非 v7/非 canonical | 拒绝 fallback，请求照常完成 |
| 标准 session/parent/body 与 client request 不同 | 保留现有高优先级值 |
| cold refresh 很慢 | 标题 RPC 立即返回空；日志 RPC 不等待 |
| expired refresh 很慢 | 标题 RPC 立即返回上一代；至多一个 worker |
| 顶层扫描/DB 错误 | 保留上一完整代；无历史时为空 |
| 单文件坏/unsafe | 只跳过该条目；不读 transcript 补救 |
| worker 无法启动 | 返回当前 snapshot，清除 in-flight，五秒后重试 |
| service/容器读不到客户端 home | 合法缺失 root 时 source 为空；页面保持 session ID/未匹配降级 |
| 旧 UI/旧 service 混用 | wire shape 未变；保持现有 nullable parent 字段兼容 |

安全与隐私约束：

- 不把 OMP marker 当认证；它只缩窄可写入本地 request-log join key 的 fallback。RPC 权限仍 admin-only。
- 不记录 raw headers、Authorization、RPC token、title、cwd、prompt 或 transcript；并发/刷新错误测试使用合成 fixture。
- OMP/Pi scanner 继续逐层相对安全打开，不把含分隔符的 path 交给 Windows 单组件 API，不引入 recursive walker。
- Pi main 的既有首个 user prompt title fallback不扩大；OMP 和 child 的 transcript/private prompt 边界不变。

## 8. 可观测性、发布与回滚

- 不新增持久化 telemetry、逐文件 warning 或 title/path 日志。验证面是：新写入 SQLite 行、标题 RPC 延迟/结果、现有运行日志中是否再出现两个 requestlog RPC timeout，以及真实日志页。
- 不延长 desktop RPC timeout。若修复后仍出现 10s title read timeout，应视为 snapshot seam 仍有同步 I/O，而不是调整 client timeout。
- 两部分可独立回滚：
  1. 回退 OMP client-request fallback：新 OMP 日志重新按旧路径可能为空，但请求转发、routing、DB 和已有日志不变。
  2. 回退合并快照 scheduler：恢复当前同步 resolver 行为；无持久化状态或数据清理。若仅 worker 有问题，可先禁用后台发布并保持上一进程内 snapshot，随后回退该代码。
- 无 schema、migration、backfill、OMP/Pi 文件或配置回滚。

## 9. 拒绝的替代方案

1. **提高 Tauri 10s timeout**：只延长 UI 卡住并加剧 RPC 连接排队，未修复 service caller 上的磁盘扫描。
2. **每个 source 各自建后台刷新器**：产生三套并发/错误/发布时间语义，调用方可能看到跨 source 半代结果；一个合并单飞快照更深、更一致。
3. **gateway 在本地会话目录查 `x-client-request-id` 是否存在**：把文件 I/O 带入请求热路径，并耦合 gateway 与 title read model；拒绝。
4. **接受任意 UUID-shaped client request ID**：普通客户端也会发 UUID request ID，可能造成错误关联；必须同时满足 OMP path、originator、UA 与 UUIDv7 合约。
5. **启动时同步 warmup**：只能移动第一次阻塞，不能解决五秒过期刷新；按需后台刷新已覆盖 cold 和 expired。
6. **持久化标题快照或 parent 关系**：需要 schema/migration、更新/删除语义和更大隐私责任，超出 PRD。
7. **新增通用 cache crate/trait adapter**：当前只有一个生产实现；具体的内部 state object 与测试注入 closure 足以证明单飞，无需新依赖或公共抽象。

## 10. 未解决的技术证据与实施风险

1. **尚无可安全持久化的实时原始 header capture**：已安装 OMP 18.0.4 源码明确发送标准 session/conversation header，但实机新日志仍为空；当前证据不能证明这些 header 在桌面反代/配置链的哪一层丢失。实现必须通过新包的真实 OMP 请求 smoke 证明严格 fallback 实际命中；不得在设计中声称已定位 header 丢失层。
2. **背景线程与测试环境隔离**：全局 worker 可能越过单个 Rust test 生命周期。并发状态测试应实例化私有 cache object，并用 barrier/channel 控制 refresh；涉及全局 public resolver 的测试必须使用现有 test env guard、合成 root，并等待/重置 worker，不能扫描真实 home 或用 sleep 猜测完成。
3. **顶层错误分类改造**：现有 scanner 大量采用局部 `Option` 降级。实施必须只把 root/DB 级失败提升为 refresh error；不能把坏单文件提升为整代失败，也不能因保留旧 snapshot 放宽 no-follow 检查。
4. **快照 clone 成本**：现有返回类型要求每次 RPC 拥有 `Vec<RequestLogSessionTitle>`；内部 `Arc<[...]>` 只能缩短锁持有，不能消除最终最多 2,000 条的 clone。若实测 clone 本身异常慢，需先给出 profile 证据再改变 public返回类型；本任务不预先扩展 wire/interface。
5. **08-21 任务状态仍为 `in_progress`，但当前 worktree 已含 child resolver/UI 行为**：本任务按运行代码为准并只做兼容回归；实施前主会话应确认不与未完成的 08-21 diff 重复编辑或回退其 child ID/parent projection。

本设计可逆且不产生新的跨项目长期架构决策，因此不创建 ADR。

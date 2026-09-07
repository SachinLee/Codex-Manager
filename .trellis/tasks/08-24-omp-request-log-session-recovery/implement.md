# 实施计划：OMP 请求日志会话恢复与非阻塞标题快照

## 1. 执行前提与质量门槛

- 质量画像：`standard`。
- 需求来源：`prd.md` 的 AC-001 至 AC-004；技术语义以 `design.md` 为准。
- 本文件只规划，不授权实施。主会话在用户审阅最新规划后，仍须完成本地 Trellis Phase 1 gate，才能运行 `task.py start`。
- 当前 `implement.jsonl` / `check.jsonl` 仍是 seed-only。按任务边界，本轮不修改它们；进入执行前至少应加入：
  - `.trellis/spec/codexmanager-service/backend/logging-guidelines.md`（请求日志身份、finalizer 和隐私约束）；
  - `.trellis/tasks/07-30-omp-session-title/research/session-title-feasibility.md`（OMP header/title 元数据证据）；
  - `.trellis/tasks/08-21-request-log-subagent-session-title/research/subagent-session-title-feasibility.md`（已支持 child 布局和 parent projection）。
- 实施前先用 LSP 对 `list_request_log_session_titles`、`resolve_request_log_session_id` 及任何准备改变可见性的 UUID helper 运行 references；不得靠文本替换漏掉 Aggregate/普通调用点。
- RED 必须因当前同步刷新或缺失严格 fallback 而失败，不得因 fixture 格式、真实 home、随机 sleep 或外网环境失败。

## 2. AC 覆盖矩阵

| Acceptance Criterion | 实施/验证切片 | 主要证明 |
| --- | --- | --- |
| AC-001 OMP 日志保存经验证的会话 ID | Slice 1、Slice 3 | gateway precedence/predicate + SQLite 行 + 新桌面包真实 OMP 请求 |
| AC-002 冷刷新和并发下标题 RPC 可用 | Slice 2、Slice 3 | barrier/channel 单飞测试 + admin RPC + 两个以上 UI 轮询窗口无 timeout |
| AC-003 已匹配 main/child 正确展示 | Slice 2、Slice 3 | stale snapshot 保留 child ID/parent projection + Playwright + 实机 main/child UI/search |
| AC-004 降级不影响转发和日志页 | Slice 1、Slice 2、Slice 3 | invalid fallback 矩阵、refresh error 保留、正常日志 RPC/请求完成、桌面 smoke |

## 3. 预期变更范围

### 3.1 预计修改的生产文件

- `crates/service/src/gateway/local_validation/request.rs`
  - 给现有私有日志身份 resolver 增加规范化 path 参数和严格 OMP fallback；不改 `IncomingHeaderSnapshot`、routing 或 finalizer。
- `crates/service/src/requestlog/requestlog_session_titles.rs`
  - 在现有深模块内增加合并快照、后台单飞、顶层刷新结果分类和 testable state object；保留全部 bounded scanner 安全边界。

### 3.2 预计修改的测试文件

- `crates/service/src/gateway/local_validation/tests/request_tests.rs`
- `crates/service/src/requestlog/requestlog_session_titles_tests.rs`
- `crates/service/src/tests/lib_tests.rs`（仅补 admin RPC/list 并发可用性时）
- `apps/tests/request-logs-duration.spec.ts`（把既有 child 标题场景扩展为 cold-empty → ready snapshot，日志列表始终可用）

### 3.3 预期审查但不修改

- `crates/service/src/gateway/request/incoming_headers.rs`
- `crates/service/src/gateway/request/request_helpers.rs`
- `crates/service/src/gateway/request/request_entry.rs`
- `crates/service/src/gateway/upstream/proxy.rs`
- `crates/service/src/rpc_dispatch/requestlog.rs`
- `apps/src-tauri/src/commands/requestlog.rs`
- `apps/src-tauri/src/commands/shared.rs`
- `apps/src-tauri/src/rpc_client/transport.rs`
- `apps/src/lib/api/service-client.ts`
- `apps/src/lib/api/transport-web-commands/misc.ts`
- `apps/src/lib/api/normalize.ts`
- `apps/src/types/request-log.ts`
- `apps/src/app/logs/page.tsx`
- `apps/src/app/logs/page-cells.tsx`
- `apps/src/app/logs/page-helpers.tsx`

这些层已有正确 capture、opaque transport、nullable parent normalization、child-keyed Map 与 UI/search。测试若发现真实行为与已读代码不符，先回到规划更新变更边界；不得为了“覆盖文件”制造 pass-through 改动。

## 4. 垂直切片

### Slice 1: AC-001 + AC-004 — 严格 OMP client-request fallback 写入真实日志 join key

- **Behavior**：当 `/v1/responses` 缺标准 session/parent/body identity，但同时带 `originator: pi`、`User-Agent: omp/<version>` 和 canonical lowercase UUIDv7 `x-client-request-id` 时，新请求日志保存该 UUID；任一 marker/格式不满足时不关联。现有 session/parent/body candidate 始终优先，请求 body/header 转发、routing 与 affinity 不变。
- **Code boundary**：只改 `local_validation/request.rs` 的私有 `resolve_request_log_session_id` 与 OMP predicate；测试在同层 `request_tests.rs`。`IncomingHeaderSnapshot` 已有字段/accessor，不修改其生产实现。
- **Test seam**：从真实 `HeaderMap` 构造 `IncomingHeaderSnapshot`，通过 `resolve_request_log_session_id` 和 `RequestLogTraceContext -> Storage::list_request_logs` 观察持久化结果；不检查 helper 源码文本。
- **RED**：
  1. `request_log_session_id_uses_strict_omp_client_request_fallback`：无标准 candidate；`/v1/responses` + `pi` + `omp/18.0.4` + UUIDv7，当前 resolver 应返回 `None`，测试预期 UUID 因而失败。
  2. `omp_client_request_fallback_persists_responses_log_session_id`：把 resolver 结果写入 in-memory SQLite 并读回；当前行为为 `NULL`，预期非空而失败。
  3. 先加入一组同文件 negative/precedence cases，但只把第 1/2 项作为明确 RED：
     - 缺 originator；缺 UA；UA 为 `codex_cli_rs/...`；originator 非 `pi`；endpoint 非 `/v1/responses`；
     - 普通字符串、UUIDv4、uppercase UUID、错误 variant/version、超长值；
     - 标准 session、parent 或 body candidate 与 client request 不同，断言现有值获胜；
     - 合法 marker 但无 candidate，断言请求身份仍为 `None`。
  4. 保存失败输出：
     ```powershell
     cargo test -p codexmanager-service --lib request_log_session_id -- --nocapture
     ```
     失败必须是 resolver 忽略严格 OMP fallback，不是 HeaderValue/SQLite fixture 错误。
- **Implementation**：
  1. 给两处 `resolve_request_log_session_id` 调用传入同一个既有规范化客户端 path；fallback 只允许 exact `/v1/responses`。
  2. 保留 `session > parent > metadata` 链，在链末尾调用 `omp_client_request_session_id`。
  3. predicate 同时校验 exact `pi`、`omp/<non-empty-version>` 和 canonical lowercase RFC-variant UUIDv7；不把 `is_native_codex_client_request` 当条件，因为其任一 Codex-like header 即可成立，过宽。
  4. 只返回一个新 `String` 给 `request_log_session_id`；不通过 `with_*` 修改 snapshot，不把 fallback 写入 conversation/routing/affinity。
  5. 不修改 body candidate 的 UUID/local ID 规则，也不接受 `pck:v1:`。
- **GREEN**：
  ```powershell
  cargo test -p codexmanager-service --lib request_log_session_id -- --nocapture
  cargo test -p codexmanager-service --lib request_log_session_id_uses_client_metadata_thread_id_for_responses_log -- --nocapture
  ```
- **Validation**：运行已有 incoming-header alias、request-helper 和 request finalizer 定向测试，确认 capture/precedence 没回归：
  ```powershell
  cargo test -p codexmanager-service --lib incoming_headers -- --nocapture
  cargo test -p codexmanager-service --lib request_helpers -- --nocapture
  ```
  代码审查必须确认 `request_log_session_id` 的新增值只沿既有日志字段传递；不新增对上游请求的写操作。
- **Dependencies**：已安装 OMP 18.0.4 源码和历史 17.1.8 研究给出 marker/ID 合约；不依赖真实会话文件或 title snapshot。
- **Rollback**：移除末级 predicate 和 path 参数，恢复原三段 precedence。无 schema/数据清理；已保存的正确 UUID 日志可继续由现有标题 read model 显示。

### Slice 2: AC-002 + AC-003 + AC-004 — 发布快照立即返回，刷新后台全局单飞

- **Behavior**：cold caller 立即得到空 snapshot，expired caller 立即得到上一完整 snapshot；同一时刻最多一个 Codex+Pi+OMP refresh。成功完成后原子发布；顶层 scan error/worker spawn error 不清空上一代。stale snapshot 中 main/child 条目的 child `sessionId`、`parentSessionId`/`parentTitle`、source、cwd 和排序保持原样。
- **Code boundary**：只深化 `requestlog_session_titles.rs`；不改变 RPC adapter、wire item、Tauri/Web、normalizer、UI 或 scanner 安全预算。测试在 `requestlog_session_titles_tests.rs`，必要的 admin RPC 断言在 `lib_tests.rs`。
- **Test seam**：私有、可实例化的 `SessionTitleSnapshotCache` 是内部 seam；生产 static 使用同一实现。测试 refresh closure 由 barrier/channel 控制并返回合成 `RequestLogSessionTitle`，不扫描真实 home、不依赖 sleep 判断是否单飞。
- **RED**：在改生产逻辑前增加以下测试，并先运行定向命令：
  1. `session_title_snapshot_cold_call_returns_before_blocked_refresh`：refresh closure 已启动但阻塞；caller 在线程中必须先返回空结果。当前同步 resolver/state 尚不存在，预期失败。
  2. `session_title_snapshot_stale_call_keeps_child_projection_during_refresh`：先发布含 child ID + parent pair 的快照，强制过期，再阻塞 refresh；caller 必须立即拿到完全相同的 child 条目。
  3. `session_title_snapshot_concurrent_callers_start_one_refresh`：多个 caller 经 barrier 同时进入，refresh counter 必须为 1；所有 caller 都拿到同一代。
  4. `session_title_snapshot_success_publishes_atomically`：worker 完成前看不到新条目，completion signal 后下一 call 得到完整新代而非 source 半代。
  5. `session_title_snapshot_error_preserves_previous_generation`：refresh 返回内部 error；上一代和 generation 不变，in-flight 被清除，下一次尝试受五秒 gate 控制。
  6. `session_title_snapshot_empty_root_is_valid_but_unreadable_root_is_error`：NotFound source 可发布为空；root 顶层权限/安全打开失败不发布。平台无法稳定制造权限失败时用注入的 root-open result 验证，不放宽 Windows/Unix 检查。
  7. `session_title_snapshot_applies_limit_without_refreshing_again`：完整发布 2,000 上限，caller 只得到 requested prefix，且不额外启动 refresh。
  8. RED 命令：
     ```powershell
     cargo test -p codexmanager-service --lib session_title_snapshot -- --nocapture
     ```
- **Implementation**：
  1. 把当前 `list_request_log_session_titles` 的 Codex读取 + external merge 移入一个返回 `Result<Vec<RequestLogSessionTitle>, RefreshError>` 的后台 builder；构建固定最大 2,000 条。
  2. 增加 `SessionTitleSnapshotCache { Mutex<SnapshotState> }`；published 使用 `Arc<[RequestLogSessionTitle]>`，state 只保留 generation、in-flight、next-refresh-at 和 published。
  3. `snapshot_and_schedule` 在锁内只判断 due、clone `Arc`、设置单飞标志；释放锁后 spawn 命名 blocking worker，再从 `Arc` clone requested prefix 返回。
  4. worker 在锁外执行所有 DB/目录/文件工作；成功时一次替换 snapshot，失败时不改 published；两者均在完成时清除 in-flight 并从完成时刻开始五秒 interval。
  5. `thread::Builder::spawn` 失败必须回滚 in-flight 并设置下一尝试时间；public resolver仍返回 `Ok(current_snapshot)`。
  6. 顶层 scanner 结果区分：root NotFound = valid empty；root/DB open/enumeration error = refresh error；descendant/file/budget/symlink/reparse 继续当前局部降级。不得在 worker 内读取第三行 OMP transcript、Pi child prompt 或未知深度。
  7. 保留现有 path+mtime+size per-file cache，作为后台 builder 的增量实现；删除 caller 上的 synchronous expired scan，不在各 source 增加另一个 worker。
  8. 测试专用 cache instance、clock/refresh closure 仅为内部确定性测试服务；不要公开 trait、依赖或第二个 resolver interface。
- **GREEN**：
  ```powershell
  cargo test -p codexmanager-service --lib session_title_snapshot -- --nocapture
  cargo test -p codexmanager-service --lib requestlog_session_titles -- --nocapture
  cargo test -p codexmanager-service --lib admin_actor_can_list_request_log_session_titles -- --nocapture
  ```
- **Validation**：
  1. 既有 `omp_title_*`、`pi_title_*`、subagent parent refresh/prune、Codex collision/limit、unsafe layout tests 全部通过。
  2. admin RPC 第一次调用仍返回 JSON array（可为空），member 仍 permission denied。
  3. 若增加 RPC 级并发测试：让 title refresh closure 阻塞时并发调用 `requestlog/sessionTitles` 与 `requestlog/list_with_summary`，两者都必须在不释放 refresh barrier 的情况下返回；不得用增加 TCP timeout 使测试通过。
- **Dependencies**：Slice 1 不阻塞此 slice，但最终发布前两者必须共同通过。依赖标准库 `Arc/Mutex/thread` 和现有 scanner，无新 crate。
- **Rollback**：移除合并 snapshot/scheduler，让 public resolver恢复同步 build；保留原 OMP/Pi增量 cache 和全部 child projection。无持久化清理。若只发生 refresh publish bug，先回退本 slice，不回退 Slice 1 的日志身份。

### Slice 3: AC-001 至 AC-004 — 跨 RPC/UI/桌面包真实 surface 验收

- **Behavior**：日志页在 cold snapshot 时仍加载日志列表，后台标题完成后在后续 poll 显示 OMP/Pi main/child 标题；child 行仍以 child ID join、显示 parent title + `子线程` Badge，Tooltip/search 保持现有语义。新 OMP 请求写入真实 session ID；invalid candidate 或标题失败只局部降级。运行日志不再出现标题 read timeout 继发日志列表 connect timeout。
- **Code boundary**：优先只扩展 `apps/tests/request-logs-duration.spec.ts` fixture/assertions；已检查的前端生产链通常无需修改。桌面/Tauri 生产文件只做编译和真实运行验证。
- **Test seam**：Playwright 通过真实 `/api/rpc` Web transport、service client、normalizer 和 React 页面；fixture 让 `requestlog/sessionTitles` 首次立即返回空、下一轮返回现有 main/child payload，同时持续记录 `requestlog/list_with_summary` 调用。
- **RED**：
  1. 在 Slice 2 实现前先建立 Rust + Playwright vertical regression：blocked/expired service refresh 的 Rust 测试应失败；Playwright 证明页面对 empty→ready 已兼容且日志列表从第一轮可见。
  2. 保留 PRD 实机基线为非自动 RED：新 OMP 行 `session_id=NULL`，且 06:56–06:58 有 repeated 10s title read timeout 后 `list_with_summary` failure。不得重新伪造这份基线或把它改写为测试 fixture 的通过证据。
- **Implementation**：
  1. Playwright mock 的第一轮 title response 立即 `[]`；第二轮返回 main 与 child，child 保留自身 `sessionId` 和完整 parent pair。
  2. 断言首次 title 为空期间 request-log 行已显示且 `list_with_summary` 被调用；后续 poll 后 main/child 标题、Badge、Tooltip、主标题搜索均正确。
  3. 不因 cold empty 在 UI 增加 loading blocker、第二 Map、手工重试或更长 timeout。若当前 UI 已通过，只提交测试变化。
- **GREEN**：
  ```powershell
  pnpm -C apps exec playwright test tests/request-logs-duration.spec.ts
  pnpm -C apps run test:runtime -- tests/request-logs-layout.test.mjs tests/transport-web-commands.test.mjs tests/tauri-command-registry.test.mjs tests/i18n-page-coverage.test.mjs
  cargo test --manifest-path apps/src-tauri/Cargo.toml rpc_client::transport::tests -- --nocapture
  ```
- **Validation**：Tauri transport tests 必须继续证明普通方法使用 default timeout；不要新增 `requestlog/sessionTitles` extended-timeout 分支。随后执行第 6 节真实桌面/package smoke。
- **Dependencies**：Slice 1/2 GREEN；本机可运行新的 OMP 请求。没有可确认 child 请求时，main smoke 必做并明确记录 child 实机项未执行，Rust fixture + Playwright child 项仍是 gate。
- **Rollback**：Playwright fixture 可随对应行为回退；前端生产链预期无改动。桌面包无需数据回滚；回退 Rust 两 slice 后重新打包即可。

## 5. 最终标准画像验证顺序

本规划阶段不运行以下命令。实施完成后按失败定位成本从低到高串行执行：

```powershell
# Rust formatting and targeted behavior
cargo fmt --all -- --check
cargo test -p codexmanager-service --lib request_log_session_id -- --nocapture
cargo test -p codexmanager-service --lib session_title_snapshot -- --nocapture
cargo test -p codexmanager-service --lib requestlog_session_titles -- --nocapture
cargo test -p codexmanager-service --lib admin_actor_can_list_request_log_session_titles -- --nocapture

# Service package gate
cargo test -p codexmanager-service --lib
cargo clippy -p codexmanager-service --all-targets -- -D warnings

# Frontend/runtime/actual page
pnpm -C apps run test:runtime -- tests/request-logs-layout.test.mjs tests/transport-web-commands.test.mjs tests/tauri-command-registry.test.mjs tests/i18n-page-coverage.test.mjs
pnpm -C apps exec playwright test tests/request-logs-duration.spec.ts
pnpm -C apps run check

# Tauri adapter/default timeout and desktop compilation
cargo test --manifest-path apps/src-tauri/Cargo.toml rpc_client::transport::tests -- --nocapture
cargo check --manifest-path apps/src-tauri/Cargo.toml
```

完成 deterministic checks、Trellis `trellis-check` 和修复后，在稳定 worktree snapshot 上只派发一次 fresh-context `workflow-reviewer`。审查重点：OMP predicate 是否过宽、fallback 是否泄漏进 routing、RPC caller 是否仍有同步 I/O、single-flight race/spawn failure、旧快照错误保留、root vs local error 分类、Windows relative-open/预算、child ID/parent projection 和 default desktop timeout。

任何 material finding 修改 diff 后必须重跑受影响命令，再做新的独立 final review；不能让实现模型批准自身工作。

## 6. 桌面/package 与实机 smoke

### 6.1 构建新包

在所有测试通过后，构建包含最新 service 的 Windows NSIS 包：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\rebuild.ps1 -Bundle nsis -CleanDist
```

不得用旧 exe、仅前端 `apps/out` 或未重启的 service 做验收。记录脚本报告的实际 artifact 路径、包版本和启动时间；不在 Trellis 记录 token/Authorization/header raw values。

### 6.2 身份与 SQLite

1. 启动新包，记录 smoke 前 `request_logs` 最大 `id`。
2. 用当前 OMP main session 发起一条新的 `/v1/responses` 请求；若有可确认 child session，再各发一条 child 请求。
3. 只读查询 smoke 后的新行：
   ```sql
   SELECT id, request_path, session_id, conversation_anchor, created_at
   FROM request_logs
   WHERE id > :before_id
   ORDER BY id;
   ```
4. 验证 `/v1/responses` 的 `session_id` 等于对应真实 OMP UUIDv7；child 行是 child ID，不以标题 read model 的 parent ID覆盖。
5. 再发一个不满足 marker/UUID predicate 的本地测试请求，确认请求照常完成且不会把任意 client request ID 伪存为 session ID。

### 6.3 cold/expired RPC 并发与 UI

1. 使用测试 profile/合成 sessions root 触发一次 cold refresh；不得改用户真实 OMP/Pi 文件。并发发起 `requestlog/sessionTitles` 和 `requestlog/list_with_summary`。
2. 标题调用必须先返回 `[]` 或上一 snapshot，日志列表正常返回；等待后台完成后下一次标题调用返回完整数组。
3. 连续观察至少两个五秒 poll window：运行日志不得出现 `requestlog/sessionTitles` read timeout，也不得继发 `requestlog/list_with_summary` connect timeout。
4. 打开 `/logs/`：main 显示自身标题；可确认 child 显示主标题 + `子线程`，第二行/Tooltip 仍保留 child ID，Tooltip 含 parent ID/source/cwd；按主标题搜索同时命中 main/child。
5. 用合成 root 制造一个顶层 refresh error：页面继续显示上一 snapshot；恢复 root 后一次成功刷新再更新。坏单文件只影响该条目，其他日志和请求正常。

若不能安全制造真实 child 或顶层权限错误，必须在 `outcome.md` 精确标记未执行项及原因；Rust deterministic fixture、Playwright 和 main OMP smoke 仍不可省略。

## 7. Scope/rollback gate

实施前记录当前 worktree，完成后确认本任务没有 task-owned schema、routing、protocol 或 timeout diff：

```powershell
git diff --name-only -- crates/core/src/storage crates/core/migrations
git diff --name-only -- crates/service/src/gateway/request/incoming_headers.rs crates/service/src/gateway/upstream crates/service/src/http
git diff --name-only -- apps/src-tauri/src/commands apps/src-tauri/src/rpc_client/transport.rs apps/src/lib/api apps/src/app/logs
```

预期最后一组只有 `apps/tests/request-logs-duration.spec.ts` 在测试侧变化；上述 production paths 应为空。若存在用户原有修改，保留并区分所有权，禁止 reset/delete；若是本任务引入，先判断是否违反设计，不能静默扩大范围。

发布后需要回滚时：

1. 先回退 snapshot scheduler（若问题是 RPC/扫描），重建包；数据库和标题文件不处理。
2. 再独立回退严格 OMP fallback（若出现误关联），重建包；不删除已写入的 UUID 日志，除非用户另行批准数据操作。
3. 不修改 schema、不运行 migration down、不回填/清空 `request_logs`、不改 OMP/Pi 文件或配置、不提高 RPC timeout。

## 8. 进入执行前的剩余 gate

- `prd.md` 无阻塞产品问题，设计与执行切片已覆盖 AC-001 至 AC-004。
- 技术规划已达到 implementation-ready；仍需主会话：
  1. 处理本会话 `task.py current --source` 无 active pointer 的上下文问题，把目标任务设为当前 planning task；
  2. 按第 1 节填入真实 `implement.jsonl` / `check.jsonl` 条目并运行 task validation；
  3. 向用户展示最终规划摘要并在后续消息取得明确实施批准；
  4. 只有之后才运行 `task.py start` 并派发 `trellis-implement` 执行上述 RED/GREEN slice。

在这些 gate 完成前，任务**不应**进入 `in_progress`。

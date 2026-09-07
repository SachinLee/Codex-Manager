# 实施计划：请求日志展示 OMP/Pi 子线程的主线程标题

## 1. 执行前提

- 质量画像：`standard`。
- 需求来源：`prd.md` 的 AC-001 至 AC-004；技术约束以 `design.md` 为准。
- 当前阶段按已批准的规划进入实施；主会话将按用户内存约束串行运行所有验证命令，不并行执行单元测试。
- OMP/Pi 目录只支持设计中列出的固定布局；不得把实现扩展成任意递归 walker。
- `implement.jsonl` 与 `check.jsonl` 已配置真实 spec/research 条目，并通过 Trellis context validation。

## 2. AC 覆盖矩阵

| Acceptance Criterion | 实施切片 | 主要证明 |
| --- | --- | --- |
| AC-001 子线程显示主线程标题 | Slice 1、Slice 2 | Rust OMP/Pi layout fixture；Playwright 标题/Badge/Tooltip |
| AC-002 主线程行为不回归 | Slice 3 | legacy main payload、无 Badge、标题/ID 搜索、service 既有测试 |
| AC-003 解析受限且可降级 | Slice 1 | Rust depth/budget/header/link/cache/partial-failure fixtures |
| AC-004 返回契约跨端一致 | Slice 2、Slice 3 | Rust serialization/RPC、Web transport、normalizer、Tauri check、frontend build |

## 3. 预期变更范围

### 3.1 预计修改的生产文件

- `crates/service/src/requestlog/requestlog_session_titles.rs`
- `apps/src/types/request-log.ts`
- `apps/src/lib/api/normalize.ts`
- `apps/src/app/logs/page-cells.tsx`
- `apps/src/app/logs/page-helpers.tsx`
- `apps/src/lib/i18n/messages/en.ts`
- `apps/src/lib/i18n/messages/ko.ts`
- `apps/src/lib/i18n/messages/ru.ts`

若现有 i18n 结构要求把日志词条放入对应 section 文件，应遵循当前消息拆分规则，而不是在顶层文件复制第二份 key。

### 3.2 预计修改的测试文件

- `crates/service/src/requestlog/requestlog_session_titles_tests.rs`
- `crates/service/src/tests/lib_tests.rs`（仅当 exact RPC serialization 不能在 resolver 测试中充分覆盖）
- `apps/tests/request-logs-duration.spec.ts`
- `apps/tests/request-logs-layout.test.mjs`（只保留适合该文件的 runtime/结构回归）
- `apps/tests/transport-web-commands.test.mjs`（补标题命令映射断言时）
- `apps/tests/run-runtime-tests.mjs`（仅当新增独立 runtime test 文件时注册）

### 3.3 预期审查但不修改

- `crates/service/src/rpc_dispatch/requestlog.rs`
- `apps/src-tauri/src/commands/requestlog.rs`
- `apps/src/lib/api/service-client.ts`
- `apps/src/lib/api/transport-web-commands/misc.ts`
- `apps/src/app/logs/page.tsx`
- `apps/src/app/logs/page-sections.tsx`

这些层已经是透明调用/透传。测试不能为了“覆盖文件”而制造 pass-through 生产改动。

## 4. 垂直切片

### Slice 1: AC-001 + AC-003 - 有界发现并生成 child-to-parent 只读投影

- **Behavior**：给定已知 OMP/Pi 目录布局，resolver 返回以 child ID 为 `sessionId`、带有效 `parentSessionId/parentTitle` 的 child 条目；parent 缺失/无标题、unknown layout、坏文件、超限或 unsafe link 只跳过局部条目。parent 改名后，child 的 `parentTitle` 在一次 5 秒 cache refresh 内更新。
- **Code boundary**：只修改 `requestlog_session_titles.rs` 的内部文件描述、known-layout discovery、child header reader、cache candidate、parent join、collision/排序；测试在 `requestlog_session_titles_tests.rs`。保持 `list_request_log_session_titles(limit)` 外部接口、root/project main discovery、Codex merge 和 admin-only RPC 不变。
- **Test seam**：`list_omp_session_titles_from_root`、`list_pi_session_titles_from_root` 和 cached resolver helper；fixture 在临时目录构造真实相对布局，通过返回 read model 断言，不读取内部 HashMap。
- **RED**：
  1. 增加 `omp_subagent_session_uses_parent_title_and_child_id`：`<project>/<main>.jsonl` + `<project>/<main-stem>/<child>.jsonl`，断言 child ID 保留、`title == None`、parent ID/title 正确。
  2. 增加 Pi agent-run fixture：`<main-session-stem>/<agent-id>/run-0/session.jsonl`，断言 child parser 只需合法首行 header；另加项目级 `subagent-artifacts/<agent>_transcript.jsonl` fixture，断言它不被索引或归属。
  3. 增加 parent 缺失、parent 空标题、parent/child 同 ID、坏 child、超限首行、权限/缺失目录和 unknown deeper layout，用例应断言其他有效条目仍返回。
  4. 增加 child 后续行包含 secret/非法 UTF-8 的用例，证明 Pi child 不向后读 prompt/session_info，OMP child 不读第三行。
  5. 增加 parent-only change + forced cache expiry：child 文件不变，返回 `parentTitle` 更新；删除 parent/child 后关系剪枝。
  6. 平台可创建安全 fixture 时增加 Unix symlink 与 Windows reparse/junction 拒绝用例；不能可靠创建 Windows reparse fixture 时，保留现有 metadata/relative-open 测试 seam，并在 Windows 实机 smoke 中验证，不以放宽校验代替测试。
  7. 先运行并保存预期失败：
     ```powershell
     cargo test -p codexmanager-service --lib subagent_session -- --nocapture
     ```
     失败原因必须是当前 resolver 没有 child discovery/parent relation，而不是 fixture 格式错误。
- **Implementation**：
  1. 给最终 Rust read model 增加成组可选父字段，并确保 main 条目省略它们。
  2. 扩展内部 file descriptor，使 child 记录稳定 `parent_cache_path`；不要把包含分隔符的相对子路径传给 Windows 单组件打开函数。
  3. 先收集现有 main descriptors，再只对 file-stem 精确匹配的派生目录执行设计中列出的 OMP/Pi 固定分支。
  4. 所有 main/child 文件共享现有 `4,000 files / 16,000 directory entries` 预算；新增 descendant-directory 计数必须显式有上限且不高于 file cap。
  5. 增加独立 header-only child readers：OMP 最多两行，Pi 最多一行；child title 不进入 candidate。
  6. 缓存 raw child candidate + parent key，在响应投影时 join 当前 parent candidate；禁止把 copied parent title 固化在未变 child cache 中。
  7. 仅在 parent ID/title 与 child ID 全部有效且同 source 时生成父字段；否则跳过 child。duplicate ID 时 main 优先 child，Codex 优先级不变。
- **GREEN**：
  ```powershell
  cargo test -p codexmanager-service --lib subagent_session -- --nocapture
  cargo test -p codexmanager-service --lib requestlog_session_titles
  ```
  两条命令必须通过；新增测试必须在删除 child discovery 或 parent join 后失败。
- **Validation**：检查测试同时覆盖 OMP、Pi agent-run 两个 source、项目级 Pi artifact 明确跳过、5 秒 cache refresh/剪枝、unknown layout、文件读取边界和局部失败；确认 existing `omp_title_*`、`pi_title_*`、Codex collision/limit 测试继续通过。
- **Dependencies**：仅依赖已接受的 PRD/设计与现有安全打开实现；不依赖 schema、gateway header 或 OMP/Pi 修改。
- **Rollback**：移除 known-layout child branches、parent-key cache projection 和新增父字段初始化，保留现有 root/project main discovery 与四字段 read model；无数据清理。

### Slice 2: AC-001 + AC-004 - 父关系跨 RPC/Web/Tauri 到 UI 完整保留

- **Behavior**：新 service child payload 经 RPC、Web transport、service client 和 normalizer 后仍保留父字段；日志单元格显示主线程标题 + `子线程` Badge，Tooltip 显示 child/parent ID、层级、OMP/Pi 来源、child cwd 和既有 route anchor。旧服务四字段响应仍按普通主线程条目处理。
- **Code boundary**：Rust serialization regression；`RequestLogSessionTitle` TypeScript 类型；`normalizeRequestLogSessionTitles`；`SessionInfoCell`；i18n 消息；Playwright fixture。Tauri command、Web command descriptor 和 `serviceClient` 预期只验证不改动。
- **Test seam**：Rust `serde_json`/RPC response 是 wire seam；`apps/tests/request-logs-duration.spec.ts` 通过 `/api/rpc` mock 走真实 Web command → service client → normalizer → React 页面，验证用户可见结果。Tauri 通过透明 `serde_json::Value` 命令编译和静态 registry 测试验证。
- **RED**：
  1. Rust child serialization 断言 exact camelCase：child 包含两个父字段，main/Codex 省略父字段，`sessionId` 保持 child。
  2. 将 Playwright `requestlog/sessionTitles` fixture 增加一个 child payload，并增加对应 request log；断言页面出现主标题与 `子线程`，hover 后出现完整子线程 ID、主线程 ID、`OMP`/`Pi` 和 cwd。
  3. 保留一个不带父字段的 legacy main payload；断言主线程不出现 `子线程` Badge。
  4. 先运行：
     ```powershell
     cargo test -p codexmanager-service --lib request_log_session_title_serialization -- --nocapture
     pnpm -C apps exec playwright test tests/request-logs-duration.spec.ts
     ```
     当前实现应因父字段未序列化/被 normalizer 丢弃/无 Badge 而失败。
- **Implementation**：
  1. TypeScript 类型增加非可选的 nullable `parentSessionId`/`parentTitle`，使 normalized domain object 始终有确定 shape。
  2. normalizer 接受 camelCase/snake_case；旧 payload 填 null；只保留成组非空、parent != child、source 为 OMP/Pi 的关系，任何 malformed half-relation 同时降级为 null/null。
  3. `SessionInfoCell` 只从 normalized parent pair 派生 `isSubagent`，用 `parentTitle || title` 计算主显示标题；不从路径/source 猜测。
  4. title 行使用现有低强调度 `Badge`，保持 row height 和 title truncation；Tooltip 对 child 改用明确的“子线程 ID/主线程 ID/执行层级”标签，main Tooltip 保持现状。
  5. 同步 en/ko/ru 静态翻译，运行 i18n coverage；不新增只服务测试的 UI helper 或第二 normalizer。
  6. 核对 Tauri/Web adapters 确实 opaque pass-through。测试若发现字段被丢弃才做最小修复；不得无理由重写 adapters。
- **GREEN**：
  ```powershell
  cargo test -p codexmanager-service --lib request_log_session_title_serialization -- --nocapture
  pnpm -C apps exec playwright test tests/request-logs-duration.spec.ts
  pnpm -C apps run test:runtime -- transport-web-commands tauri-command-registry i18n-page-coverage
  ```
- **Validation**：
  ```powershell
  pnpm -C apps exec eslint src/types/request-log.ts src/lib/api/normalize.ts src/app/logs/page-cells.tsx src/lib/i18n/messages/en.ts src/lib/i18n/messages/ko.ts src/lib/i18n/messages/ru.ts
  cargo check --manifest-path apps/src-tauri/Cargo.toml
  ```
  若日志词条位于 section 文件，eslint 命令应替换为实际改动的消息文件。验证 Web 与 Tauri 仍调用同一 `requestlog/sessionTitles` 方法，且没有字段重构逻辑散落到页面。
- **Dependencies**：依赖 Slice 1 已输出稳定父关系；Playwright fixture 不依赖用户真实 OMP/Pi home。
- **Rollback**：先回退 `SessionInfoCell`/i18n，再回退 TypeScript 父字段/normalizer，最后回退 Rust serialization；任一阶段旧四字段主线程链路仍工作。无需改数据库或网关。

### Slice 3: AC-002 + AC-004 - 搜索语义、legacy compatibility 与主线程回归

- **Behavior**：会话标题搜索用与 UI 相同的 `parentTitle || title` 投影；搜索主线程标题会把对应 main ID 和 child ID 都展开到 `session_in`，搜索 child ID 仍命中 child 日志。Codex/OMP/Pi main 标题、cwd、source、无标题/未匹配降级和无 Badge 行为保持不变；malformed/legacy parent 字段不造成误标。
- **Code boundary**：`buildRequestLogSearchQuery` 的输入 projection 与 Playwright/runtime tests；必要时只调整 `page-helpers.tsx`。`page.tsx` 仍传同一 `requestLogSessions` 列表并以 `sessionId` 建 Map，不创建 parent alias Map。
- **Test seam**：在 Playwright mock 中记录 `requestlog/list_with_summary` 的 `params.query`，通过真实搜索控件触发；使用 legacy main、valid child、half-relation 和 unmatched log 验证可见行为。搜索断言面向 RPC query 和最终行，不面向 helper 源码文本。
- **RED**：
  1. 选择“会话标题”，输入 parent title，断言发出的 query 包含 main ID 与 child ID；当前只读 `title` 的 helper 会漏掉 `title:null,parentTitle:*` child，因此应失败。
  2. 输入完整 child ID，断言 query 命中 child ID。
  3. legacy main payload（缺父字段）继续显示自身标题/来源/cwd且无 Badge；valid OMP/Pi main 同样无 Badge。
  4. 只含 `parentSessionId` 或 parent==child 的 malformed payload 经 normalizer 后按普通/未匹配状态显示，不出现 Badge。
  5. 先运行：
     ```powershell
     pnpm -C apps exec playwright test tests/request-logs-duration.spec.ts
     ```
     新增 parent-title search 断言必须在旧 helper 上失败。
- **Implementation**：
  1. 将 search helper 的 session projection 扩展为可选 `parentTitle`，有效搜索标题为 `parentTitle || title`。
  2. 保持现有最多 200 IDs、去重、逗号拒绝、显式 `session_in:` passthrough 和 child `sessionId` 语义。
  3. 不把 `parentSessionId` 自动加入 child 搜索命中；本轮搜索 contract 是可见主标题或当前日志 session ID，避免扩大未批准语义。
  4. 保持 `requestLogSessionMap` key 为原始 `sessionId`，不以 parent ID 覆盖 child。
- **GREEN**：
  ```powershell
  pnpm -C apps exec playwright test tests/request-logs-duration.spec.ts
  pnpm -C apps run test:runtime -- request-logs-layout i18n-page-coverage
  ```
- **Validation**：
  ```powershell
  pnpm -C apps exec eslint src/app/logs/page-helpers.tsx src/app/logs/page-cells.tsx src/app/logs/page.tsx src/lib/api/normalize.ts src/types/request-log.ts
  pnpm -C apps run build:desktop
  ```
  浏览器中分别检查 main 与 child 行：main 没有 Badge；child 标题与 parent 一致但第二行仍是 child short ID；Tooltip 完整 ID 未串位。
- **Dependencies**：依赖 Slice 2 normalized parent pair 和 UI fixture。
- **Rollback**：回退 search helper 对 `parentTitle` 的选择即可恢复旧搜索；resolver/contract/UI 仍能展示 child，不影响日志存储或转发。

## 5. 最终验证顺序

按失败定位成本从低到高执行；本节命令在实施完成后运行，本规划阶段不运行：

```powershell
# Rust 定向行为与完整 service lib
cargo fmt --all -- --check
cargo test -p codexmanager-service --lib subagent_session -- --nocapture
cargo test -p codexmanager-service --lib requestlog_session_titles
cargo test -p codexmanager-service --lib admin_actor_can_list_request_log_session_titles
cargo test -p codexmanager-service --lib
cargo clippy -p codexmanager-service --all-targets -- -D warnings

# Frontend 定向与完整 standard gate
pnpm -C apps run test:runtime -- request-logs-layout transport-web-commands tauri-command-registry i18n-page-coverage
pnpm -C apps exec playwright test tests/request-logs-duration.spec.ts
pnpm -C apps run check

# Tauri adapter 编译兼容
cargo check --manifest-path apps/src-tauri/Cargo.toml
```

若新增 runtime test 文件，必须先将其加入 `apps/tests/run-runtime-tests.mjs`，然后把文件名加入定向命令；不能只直接运行一个未注册、完整 `test:runtime` 永远不会执行的测试。

### 浏览器/实际 surface 验收

Playwright 必须验证实际构建的日志页，而不只是测试 helper。最终再用本机有证据的 OMP/Pi 样本做 admin 日志页 smoke（目录在 service 同机可读时）：

1. 调用/观察 `requestlog/sessionTitles`，确认 child 条目保留 child ID 并带 parent ID/title。
2. 打开 `/logs/`，等待至多一个 5 秒刷新窗口；确认 child 行显示主标题 + `子线程`。
3. hover/focus child 单元格，确认 child/parent ID、来源和 cwd 正确。
4. 检查同一主线程行没有 Badge；按主标题搜索时 main/child 日志均命中。
5. 临时移除/改坏 fixture parent（仅测试目录，不动用户会话）后刷新，确认页面局部降级且日志请求/转发仍正常。

若运行环境没有可安全使用的真实 OMP/Pi 样本，必须明确记录“真实本机 smoke 未执行”，但 Playwright fixture 和 Rust layout tests仍为必需门槛。

## 6. 显式 no-schema / no-gateway scope gate

实施前记录现有 worktree，最终只审查本任务拥有的 diff；不得删除或重置用户已有修改。

以下路径对本任务必须无 task-owned diff：

```powershell
git diff --name-only -- crates/core/src/storage crates/service/src/gateway
```

预期输出为空。若非空，先区分既有用户修改与本任务修改；任何本任务对这些路径的改动都必须停止并回到规划，不得以“顺手保存 parent header”继续。

同时检查透明 adapter/dispatch 文件：

```powershell
git diff --name-only -- crates/service/src/rpc_dispatch/requestlog.rs apps/src-tauri/src/commands/requestlog.rs apps/src/lib/api/transport-web-commands/misc.ts apps/src/lib/api/service-client.ts
```

预期同样为空；只有测试证明现有透传会丢字段时，才允许在重新评审设计后做最小修复。无论如何都禁止：

- 新增/修改 `request_logs` column、migration、索引或回填；
- 修改 `x-openai-subagent`、`x-codex-parent-thread-id` 或 session ID precedence；
- 修改 HTTP/WebSocket/Aggregate forwarding/finalizer；
- 修改 OMP/Pi 文件或协议。

## 7. 独立检查与回滚门槛

完成所有 slice 后：

1. 运行 Trellis `trellis-check`（标准画像）并修复 findings；修复后重跑受影响命令。
2. 在 worktree snapshot 稳定后运行一次 fresh-context `workflow-reviewer`，重点审查：路径深度/预算、Windows relative open、parent cache invalidation、wire compatibility、child ID 保留、隐私读取边界和 UI 搜索一致性。
3. 若 reviewer 发现需要改 schema/gateway/协议才能成立，停止实施并退回 Phase 1；不得静默扩大范围。
4. 任一性能/安全问题可先只回退 child discovery；四字段 main title read model、数据库和网关无需回滚。

## 8. 当前执行状态

- PRD、设计、实施计划和界面决策已批准；任务已进入 `in_progress`。
- 第一阶段只实施 Rust resolver、父关系缓存投影和对应回归测试；TypeScript/UI/search/i18n 属于后续阶段。
- 实施代理启动两次均因 OMP 内部 `settingsOverride` TypeError 在启动前失败，未产生代码修改；主会话接管实现。
- 所有测试、构建、lint 和质量检查必须由主会话按串行顺序执行，避免内存压力。

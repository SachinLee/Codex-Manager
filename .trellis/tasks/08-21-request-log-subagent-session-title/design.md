# 技术设计：请求日志展示 OMP/Pi 子线程的主线程标题

## 1. 质量画像与设计目标

- **质量画像：`standard`**。本任务改变 Rust 只读 read model、`requestlog/sessionTitles` 返回契约、TypeScript 归一化和日志页可见行为，属于常规跨层行为变更；不涉及认证/授权、资金、secret、持久化数据迁移或破坏性操作。
- 目标对应 AC-001 至 AC-004：请求日志仍以 child `sessionId` 为 join key，但在可确认父关系时显示主线程标题和 `子线程` Badge；主线程行为不变；所有文件发现都必须有界并局部降级。
- 最小变更原则：深化现有 `requestlog_session_titles` 模块，保持 `list_request_log_session_titles(limit)` 这一条外部接口，不引入第二套标题服务、数据库投影或 UI 路径推断。

## 2. 当前行为

当前数据流为：

```text
Codex DB + OMP/Pi local JSONL
  -> crates/service request-log session title resolver
  -> admin-only requestlog/sessionTitles RPC
  -> Tauri command 或 Web command descriptor（JSON 透传）
  -> serviceClient + normalizeRequestLogSessionTitles
  -> sessionId -> RequestLogSessionTitle Map
  -> SessionInfoCell + 会话标题搜索
```

现有模块的关键性质：

1. `RequestLogSessionTitle` 只有 `sessionId/title/cwd/source`；`sessionId` 是日志页唯一 join key。
2. OMP/Pi 扫描仅收集 sessions root 的直接 JSONL 和一层项目目录的直接 JSONL，不能表达 child 到 parent 的关系。
3. OMP 解析只读固定上限内的 title slot 与 session header；Pi 主会话解析在固定条目数和单条字节上限内读取 `session_info.name`，并保留首个 user prompt 兜底。
4. 文件缓存按路径、mtime、size 复用结果，刷新窗口为 5 秒；缺失目录、坏文件和权限错误均返回局部结果。
5. Windows 使用已打开目录句柄相对打开单个 basename，Unix 使用 no-follow 打开；两端都拒绝 symlink/reparse point。
6. Tauri 与 Web transport 不解释标题条目，只转发同一个 RPC JSON；前端 normalizer 是不可信 JSON 到 typed read model 的唯一解码 seam。
7. `SessionInfoCell` 当前显示标题、短会话 ID、来源和工作目录；`buildRequestLogSearchQuery` 使用同一列表将标题查询展开为 `session_in:<ids>`。

## 3. 变更边界

### 3.1 范围内

- 扩展现有 Rust resolver 的内部文件描述、缓存候选和最终只读投影。
- 仅识别下文列出的 OMP/Pi 已知父子布局。
- 对 `requestlog/sessionTitles` 增加可选父关系字段。
- 同步 TypeScript 类型、normalizer、会话标题搜索和 `SessionInfoCell`。
- 增加 Rust fixture、RPC/serialization、frontend runtime/transport、Playwright 和 i18n 覆盖。

### 3.2 明确不变

- 不修改 SQLite `request_logs` schema、migration、历史数据或 `RequestLog.session_id`。
- 不修改 `crates/service/src/gateway/**`、请求头解析、session ID 优先级、转发、HTTP/WebSocket/Aggregate finalizer。
- 不修改 OMP/Pi 协议、客户端、私有 header 或会话文件。
- 不把 OMP/Pi 数据接入可变的 `codexSession/*` 接口。
- 不远程读取客户端文件，不上传或持久化标题，不展示 child prompt/transcript/tool call/model context。

## 4. Resolver 数据模型与深模块接口

### 4.1 对外 read model

保留现有字段并增加一对可选、成组出现的父关系字段：

```rust
#[serde(rename_all = "camelCase")]
struct RequestLogSessionTitle {
    session_id: String,
    title: Option<String>,
    cwd: Option<String>,
    source: RequestLogSessionSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_title: Option<String>,
}
```

接口不新增 `isSubagent` 或 `parentSource`：

- `isSubagent` 可由有效 `parentSessionId` 派生；重复传布尔值会产生不一致状态。
- 已知布局中 parent 与 child 必须属于同一个 OMP/Pi source，因此 `parentSource` 与 `source` 重复。

### 4.2 字段语义和不变量

| 字段 | 主线程 | 已确认子线程 |
| --- | --- | --- |
| `sessionId` | 主线程 ID | **child ID，绝不替换为 parent ID** |
| `title` | 当前主线程自身标题 | `null`；不提取/暴露 child 自身标题 |
| `cwd` | 主线程 cwd | child session header 中的 cwd；缺失时不伪造 parent cwd |
| `source` | `codex/omp/pi` | `omp/pi` |
| `parentSessionId` | 不序列化 | 主线程 ID |
| `parentTitle` | 不序列化 | 主线程当前有效标题 |

父字段必须满足以下不变量：

1. `parentSessionId` 和 `parentTitle` 同时存在或同时不存在。
2. 两者只在路径形态、parent main file、parent ID、parent title 和 child ID 都通过验证时出现。
3. `parentSessionId != sessionId`，且 parent/child source 相同。
4. UI 的有效标题为 `parentTitle ?? title`；搜索使用同一投影，避免显示与搜索语义分叉。
5. 无有效 parent title 时不发布半关系；该 child 条目被跳过，日志页沿用“未匹配会话”降级，而不是猜测 child title。

示例 wire payload：

```json
[
  {
    "sessionId": "019f-parent",
    "title": "修复登录超时",
    "cwd": "D:/work/example",
    "source": "omp"
  },
  {
    "sessionId": "019f-child",
    "title": null,
    "cwd": "D:/work/example",
    "source": "omp",
    "parentSessionId": "019f-parent",
    "parentTitle": "修复登录超时"
  }
]
```

示例中的 ID 仅说明字段关系；实际 OMP/Pi ID 仍必须通过现有 UUID-shaped normalization。

### 4.3 内部候选与缓存

缓存不能只存复制后的 `parentTitle`，否则 parent 改名而 child 文件未变时会返回旧标题。内部候选应保留：

- child 自身解析结果（ID、cwd、source、mtime）；
- 可选的稳定 `parent_cache_path`/parent file key；
- 不包含 child prompt 或 child title。

每次从缓存生成返回列表时，以 parent file key 关联当前缓存中的 main candidate，再构造最终 `parentSessionId/parentTitle`。因此：

- parent 标题在 5 秒缓存过期并重读后，会同步更新所有关联 child；
- child 未变化时仍复用其 header 解析结果；
- parent 删除、改成空标题或解析失败后，关联 child 在同一刷新中消失；
- child 删除后照现有 cache replacement 规则剪枝。

`list_request_log_session_titles(limit)` 仍是调用方和测试的外部 seam；路径分类、布局识别和 parent join 都隐藏在该深模块内部。

## 5. 有界 OMP/Pi parent discovery

### 5.1 通用发现策略

扫描分成“发现 main 文件”与“从 main 文件名识别已知派生目录”两步，不使用 recursive walker：

1. 按现有规则发现 sessions root 直接 main JSONL，以及 root 的一层 project directory 中的直接 main JSONL。
2. 在同一 container 内，只有当普通目录 basename **精确等于一个已发现 main JSONL 的 file stem** 时，才把它视为该 main 的派生目录。
3. child 描述记录 parent main file 的稳定 cache path；parent ID/title 来自 parent 文件内容，不从目录名或 child payload 猜测。
4. 只按下述固定路径形态继续打开目录。任何额外层级、未知名称或不匹配的祖先都停止，不做递归兜底。

`container` 仅表示 sessions root 或其已安全打开的一层 project directory。

### 5.2 支持的 OMP 布局

```text
<container>/
  <main-session>.jsonl
  <main-session-stem>/
    <direct-child>.jsonl
```

- 派生目录只收集直接 regular `.jsonl`；不进入它的任何子目录。
- child 解析只读取固定上限内的 OMP title slot 和 session header，以验证文件形态并取得 child ID/cwd；title slot 内容不进入返回模型。
- 派生目录必须与同 container 的 main file stem 精确匹配。

### 5.3 支持的 Pi 布局

```text
<container>/
  <main-session>.jsonl
  subagent-artifacts/<agent>_transcript.jsonl  # 项目级聚合转录，明确不支持
  <main-session-stem>/
    <agent-id>/
      run-<decimal>/
        session.jsonl
```

- agent run 只接受单一普通 path component 的 `<agent-id>`、名称匹配 `run-<decimal>` 的直接子目录，以及 basename 精确为 `session.jsonl` 的 regular 文件。
- Pi child parser **只读取首个 bounded session header**；不扫描 `session_info`、user message 或后续 transcript。若运行文件不以合法 session header 开始，则该 child 不受本轮支持并被局部跳过。
- 项目级 `subagent-artifacts/<agent>_transcript.jsonl` 与主会话不共享可验证路径祖先，文件名也不包含 parent session ID；不得按 cwd、时间或 agent 名猜测归属，本轮明确跳过。
- parent main 文件继续沿用既有、有限条目/单条字节的 Pi title 规则，以保持主线程标题行为。

### 5.4 明确不支持的布局

以下情况不做猜测：

- 任意深度 `**/*.jsonl`、递归追踪、从 child JSON 内任意 parent 字段跳转；
- 派生目录名与 main file stem 不相同；
- OMP child 位于派生目录的孙目录；
- Pi agent run 不满足上述固定名称与层级，或项目级 `subagent-artifacts/` 转录文件；
- 远程主机、容器未挂载目录或其他用户 home；
- 仅能解析 child ID、但找不到有效 parent ID/title 的半关系。

未知布局统一退化为既有“未匹配会话/无标题”体验。未来出现新布局时，必须先补真实布局证据、独立 fixture 和新的显式路径分支，不能放宽为任意递归。

### 5.5 预算与路径安全

扩展发现必须复用并统一消费现有预算，而不是给每层重新计数：

- 所有 main + child JSONL 合计不超过 `MAX_OMP_SESSION_FILES = 4_000`。
- 所有被枚举目录的 directory entries 合计不超过 `MAX_OMP_DIRECTORY_ENTRIES = 16_000`。
- root 的直接 project directory 仍不超过 `MAX_OMP_PROJECT_DIRECTORIES = 512`。
- 新增 descendant-directory 计数上限不得高于文件上限；每次尝试打开 main-derived、agent 或 run 目录都同时消费共享 entry 预算。
- OMP child 每个文件最多读取 `MAX_OMP_TITLE_SLOT_BYTES + MAX_OMP_SESSION_HEADER_BYTES`。
- Pi child 每个文件最多读取 `MAX_PI_SESSION_HEADER_BYTES`；不调用读取 4,096 条 entry 的 main-session parser。
- title/session ID/cwd 继续使用现有字节上限和 control-character 校验。
- 5 秒 refresh interval 不变；不得为 child 再叠加第二个独立轮询器或缓存。

每层目录都必须通过现有 no-follow/reparse-safe 打开：

- Unix：`symlink_metadata` 校验、`O_NOFOLLOW`/目录句柄语义；
- Windows：拒绝 `FILE_ATTRIBUTE_REPARSE_POINT`，通过已验证 parent directory handle 相对打开单一 basename；
- 不把带分隔符的相对子路径传给 Windows 单组件打开接口；每一层逐个安全打开。

达到任一预算时停止继续发现，但保留此前已验证候选并成功返回 RPC。

## 6. 合并、排序与冲突规则

- 保持 Codex 对同 ID external candidate 的现有优先级。
- 同一 external source 内，main candidate 优先于 child candidate，防止重复 ID 被错误标成子线程。
- OMP/Pi 间现有冲突优先级和最终 updated-at 排序保持不变；child 使用 child 文件 mtime 参与排序。
- 最终 `limit` 与 `MAX_SESSION_TITLE_LIMIT = 2_000` 不变；新增 child 与 main 共享返回上限，不引入无上限旁路。
- 未确认 parent 的 child 不参与最终列表；一个坏 child 或坏 parent 不应移除其他有效 main/child。

## 7. 精确跨层契约

### 7.1 Service/RPC

- RPC 方法、参数、权限不变：`requestlog/sessionTitles({ limit? })`，仅 admin 可调用。
- 结果仍是 `RequestLogSessionTitle[]`，仅对确认的 child 追加 camelCase `parentSessionId` 和 `parentTitle`。
- main/Codex 条目的 JSON shape 保持原样，父字段因 `skip_serializing_if` 缺省。
- 不修改 `rpc_dispatch/requestlog.rs` 的权限或方法分派；只增加 serialization/response regression coverage。

### 7.2 Tauri/Web adapters

- `service_requestlog_session_titles` 继续把 RPC `serde_json::Value` 原样返回。
- Web command `service_requestlog_session_titles -> requestlog/sessionTitles` 映射不变。
- 两个 adapter 都不得重命名、筛选或重新构造条目；通常无需修改生产代码，只需验证映射和编译链。

### 7.3 TypeScript normalization

`RequestLogSessionTitle` 增加：

```ts
parentSessionId: string | null;
parentTitle: string | null;
```

`normalizeRequestLogSessionTitles` 是唯一 JSON 解码 owner：

1. 同时接受 camelCase 与 snake_case 父字段。
2. 对旧服务缺字段填 `null`。
3. 只有当两个父字段都是非空字符串、parent ID 与 child ID 不同、source 为 OMP/Pi 时才保留关系；否则两者同时归一为 `null`。
4. 不从文件路径、`title` 或 source 在 UI 端推断 child。

旧前端会忽略新增 JSON 字段；新前端读取旧服务响应时得到 `null/null`，因此按主线程/普通条目显示。

### 7.4 日志页面数据流

- `requestLogSessionMap` 仍以 child `sessionId` 为 key，不以 parent ID 重键，也不创建别名 Map。
- `SessionInfoCell` 计算：
  - `isSubagent = Boolean(parentSessionId && parentTitle)`；
  - `displayTitle = parentTitle || title || 既有降级文案`。
- `buildRequestLogSearchQuery` 对每个条目使用同样的 `parentTitle || title`；搜索主标题时会同时展开匹配的 main ID 与 child ID，搜索 child ID 仍命中 child 日志。
- 不把 child 自身 title 纳入展示或搜索。

## 8. UI 设计

### 8.1 表格单元格

主线程保持当前两行：标题 + `session: <short-id>`，不显示额外 Badge。

确认的 child：

```text
<主线程标题>  [子线程]
session: <short-child-id>
```

- 使用现有 `Badge`，低强调度、小字号、`shrink-0`；标题继续截断，Badge 不改变表格行高。
- Badge 文本本身提供可访问语义，不仅依赖颜色。

### 8.2 Tooltip

主线程 Tooltip 保持现有“会话标题 / 会话 ID / 来源 / 工作目录”。

child Tooltip 显示：

- 会话标题：主线程标题；
- 执行层级：子线程；
- 子线程 ID：当前日志的完整 child ID；
- 主线程 ID：`parentSessionId`；
- 来源：OMP 或 Pi；
- 工作目录：child header 的 cwd（存在时）；
- 既有 conversation anchor（存在时）。

新增静态中文 key 必须同步 `en/ko/ru` 消息，遵守现有 i18n coverage；不硬编码英文 fallback。

## 9. 失败模式与局部降级

| 失败 | 行为 |
| --- | --- |
| sessions root 缺失/不可读 | 该 source 返回空，RPC 继续返回其他 source |
| main JSON 损坏、ID 非法、标题空 | main 按既有规则处理；不发布其 child 关系 |
| derived 目录缺失/名称不匹配 | 只返回 main；child 日志未匹配 |
| child header 损坏、超限或 ID 非法 | 只跳过该 child |
| symlink/reparse point/路径组件异常 | 拒绝该文件或目录，不跟随 |
| directory/file/byte budget 用尽 | 停止继续发现，返回此前有效候选 |
| parent 改名或删除 | 最迟下一次 5 秒 refresh 更新/移除 child 关系 |
| Pi agent-run 文件不是 session-header JSONL | 跳过，不读取后续内容寻找 ID |
| 旧服务缺少父字段 | normalizer 填 null，保持普通会话行为 |
| 远程/容器无法访问本机 OMP/Pi home | 保持 session ID 与“未匹配会话”，日志页和转发不失败 |

不应把预期的单文件跳过提升为整个 RPC error，也不应通过读取更多 transcript 来“修复”坏元数据。

## 10. 安全与隐私

- admin-only RPC 权限不变；不扩展成员可见面。
- 文件访问只读、no-follow、固定路径形态、固定深度、固定数量和固定读取字节。
- parent 关系来自本地目录结构 + 两端合法 header，不信任请求头或 child transcript 声明。
- OMP child 虽需读取前两条结构行，但不返回 title slot；Pi child 只读 header，绝不读取 child prompt/session_info。
- Pi main 的 prompt fallback 是现有行为，继续受 entry/byte/title normalization 上限约束；本任务不扩大该读取范围。
- 不把标题、路径、ID 写入新的数据库表、请求日志正文或额外诊断日志；不新增包含本地路径/标题的 telemetry。
- UI 仅显示既有 cwd 加 parent ID/title；不展示 child prompt、agent 名、artifact 名或工具调用。

## 11. 可观测性

本功能是本地只读投影，不增加持久化指标或逐文件 warning，避免路径/标题泄露及坏文件日志风暴。可观察面为：

- `requestlog/sessionTitles` 中 child 条目的成组父字段；
- 现有 5 秒刷新后关系的更新/剪枝；
- Rust fixture、serialization、normalizer、runtime transport 与 Playwright 行为证据。

如果未来需要扫描诊断，应只增加聚合计数且单独设计，不在本任务顺带引入。

## 12. 兼容性、发布与迁移

- **无数据库迁移、无数据回填、无网关迁移。**
- wire 变更是新增可选字段：旧 UI 忽略；新 UI 对旧 service 缺字段归一为 null。
- 推荐 service 与 UI 同一版本发布。若分阶段，优先 UI normalizer/UI，再部署 service；反向部署时旧 UI 最多把新发现 child 显示为无标题，不会改写日志 ID 或影响转发。
- Tauri 与 Web 使用同一 RPC JSON，必须在两种 runtime 下完成构建/transport 验证。
- 不设 feature flag：该投影无持久化副作用，且各层可独立回退；新增 flag 只会扩大状态空间。

## 13. 方案比较

### 13.1 采用：现有 resolver 内的已知布局父子投影

以最小外部接口集中 discovery、cache、join、normalization 和降级，调用方只消费 typed projection，具有较高 Depth 和 Locality。

### 13.2 拒绝：修改 SQLite 保存 parent/subagent/title snapshot

需要 schema migration、所有日志 finalizer 同步、历史回填和标题改名语义，并扩大隐私保留责任；不符合只读投影目标。

### 13.3 拒绝：从请求头或 UI 推断 parent

现有持久化日志没有这些 header 字段；头语义不能证明本地文件 parent，无法覆盖历史日志。UI 读取路径/JSONL还会把解析逻辑泄漏到错误 seam。

### 13.4 拒绝：展示 child 自身标题或 prompt

不满足主线程标题目标，OMP child 常为空，并扩大子任务文本暴露和搜索噪音。

### 13.5 拒绝：任意递归扫描 `**/*.jsonl`

外部目录可漂移，递归会扩大 I/O、链接攻击面和 transcript 误读概率；已知布局的显式分支已足够满足当前证据。

### 13.6 拒绝：同时返回 `isSubagent` 和 parent 字段

布尔值可从完整 parent 关系派生，重复状态可能出现 `isSubagent=false` 但 parent 非空等非法组合。

本决策可逆且未引入跨项目标准，因此不创建 ADR。

## 14. Rollout 与 rollback

按垂直切片发布：resolver/contract → typed normalization → UI/search。每片必须先有 RED，再有 GREEN。

独立 rollback 点：

1. UI 有问题：回退 `SessionInfoCell` 与搜索对 parent 字段的消费；service 新字段会被忽略。
2. normalizer/contract 有问题：回退父字段类型和 serialization；现有四字段 main 契约仍可工作。
3. nested discovery 有性能或平台问题：移除 known-layout child 分支和 child cache relation；保留现有 root/project main 扫描。

所有 rollback 都无需回滚数据库、清理历史行或恢复网关配置。

## 15. 开放技术风险

无阻塞产品决策；实施时仍需控制以下技术风险：

1. **外部布局漂移**：OMP/Pi 目录是实现细节，不是 CodexManager 协议。当前只承诺研究中已观察的固定布局，未知布局按未匹配降级。
2. **Pi agent-run 头格式差异**：某些运行文件可能不以 `type=session` header 开头；本轮不会向后扫描正文寻找 ID，因此这些文件会被安全跳过。
3. **Windows 竞态与 reparse point**：新增的每一层必须继续通过 parent handle 相对打开，不能退回 canonical path 拼接后直接 open。
4. **返回上限竞争**：child 与 main 共享 2,000 条结果上限；需要用确定性排序和 main-over-child collision 规则避免错误标记，并在 fixture 中覆盖。
5. **parent 依赖缓存失效**：若 finalized parent title 被缓存到 child 而非响应时 join，会出现 parent 改名后 child 标题滞后；实现必须按本设计保存 parent key 并重新投影。
6. **跨端字段丢失**：Tauri/Web adapter 当前是 opaque JSON 透传，主要风险在 TypeScript normalizer；必须用旧/新 payload 和两种 transport/build 证据确认。

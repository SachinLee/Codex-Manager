# 背景与现状

请求日志页面当前在 `apps/src/app/logs/page.tsx` 中维护一个 `searchField + search` 复合筛选状态，并由 `buildRequestLogSearchQuery` 转成单个前缀查询：模型使用 `model:<value>`，标题先查 session titles 再转成 `session_in:<ids>`。`apps/src/app/logs/page-sections.tsx` 在筛选展开时渲染该复合控件和右侧统计卡片。

服务端 `RequestLogListParams` 目前只有 `query`、状态和时间范围；`crates/core/src/storage/request_log_filters.rs` 每次只解析一个 `RequestLogQuery`，无法表达标题、模型、平台秘钥三者的 AND 组合。`RequestLogFilterSummaryResult` 目前只返回 Token 总量和费用，底层 `request_log_summary_sql` 也未返回筛选结果的输入 Token与缓存输入 Token聚合值。

# 方案与改动边界

采用“显式参数 + 服务端 AND 过滤 + 现有结果查询复用”的方案：

1. 前端拆分为三个独立状态和控件：
   - 标题：`Input`，复用现有 session title lookup，将标题/父标题/会话 ID匹配结果转换为 `titleSessionIds`（空输入不传；有输入但无匹配则传一个不会命中的哨兵 ID）。
   - 模型：`Select`，选项来自 `StartupSnapshot.apiModels.models`（平台模型目录），值使用模型 `slug`，增加“全部模型”空值。
   - 平台秘钥：`Select`，选项来自现有 `apiKeysResult`/启动快照，值使用 `ApiKey.id`，增加“全部平台秘钥”空值。
2. 扩展 `RequestLogListParams` 与前端 service client 参数为显式字段：`session_ids`/`sessionIds`、`model`、`key_id`/`keyId`。保留 `query` 以兼容现有全局搜索和其他调用者；本页面不再生成单一复合查询。
3. 在 `crates/core/src/storage/request_log_filters.rs` 将显式字段追加到同一 WHERE 子句：标题会话 ID 使用 `IN`，模型使用 `r.model = ?`，平台秘钥使用 `r.key_id = ?`。列表、计数、汇总、按模型汇总共用同一过滤器，保证结果和统计一致。显式字段与 `query` 同时存在时使用 AND。
4. 在筛选汇总结果增加 `input_tokens` 与 `cached_input_tokens`，由同一筛选 SQL 聚合；前端用已有 `formatCacheRate(inputTokens, cachedInputTokens)` 显示 `cached/input` 的比例。输入 Token为零时显示 `-`，缓存 Token被格式化函数限制在输入 Token范围内。
5. 右侧统计模块新增一个与现有 `SummaryCard` 同风格的“缓存率”卡片；现有请求数、Token、费用卡片保留。小屏继续两列布局，大屏由现有 grid 自适应，不新增独立图表或全局统计请求。

# 组件与职责

- `apps/src/app/logs/page.tsx`：筛选状态、查询 key、API 参数映射、模型目录读取、空结果占位条件、清空日志后的 summary 清零。
- `apps/src/app/logs/page-sections.tsx`：三个独立控件和缓存率统计卡片的展示；通过 props 接收值和回调，不在展示层组装查询语法。
- `apps/src/app/logs/page-helpers.tsx`：保留标题 ID 解析 helper；删除不再需要的 `SearchField`/单一字段 placeholder 逻辑，或改为窄职责的标题解析 helper。
- `apps/src/lib/api/service-client.ts`：为 list/list-with-summary/summary 三个请求传递显式筛选字段。
- `apps/src/lib/api/transport.ts`：如存在重复的 Web RPC 参数定义，保持字段透传；不新增 raw fetch。
- `apps/src/types/request-log.ts`：补充列表参数对应的汇总 Token字段。
- `crates/core/src/rpc/types.rs`：扩展 `RequestLogListParams` 与 `RequestLogFilterSummaryResult`，保持 `serde(rename_all = "camelCase", default)` 兼容旧客户端字段缺失。
- `crates/core/src/storage/request_log_filters.rs`：构造显式字段的 SQL 条件。
- `crates/core/src/storage/request_log_query.rs`：仅保留兼容 `query` 的既有语法；不把多个 UI 字段拼成字符串。
- `crates/core/src/storage/request_logs.rs`：汇总 SQL、`RequestLogQuerySummary` 映射及所有调用链传递新字段。
- `crates/service/src/requestlog/requestlog_list.rs`、`requestlog_summary.rs`：归一化并传递新参数，映射新汇总字段。
- `apps/tests/request-logs-duration.spec.ts`：扩展 RPC fixture 和 UI 断言，证明独立控件、组合参数和缓存率展示。
- `crates/core/src/storage/tests/request_logs_tests.rs`、`crates/service/src/requestlog/requestlog_list_tests.rs`：增加组合过滤及空输入边界测试。

# 公开接口与不变量

- `RequestLogListParams` 新字段均为 `Option`/带默认值；旧调用者只传 `query` 时行为不变。
- `session_ids` 为空表示不加标题条件；非空表示只匹配这些会话 ID。模型/平台秘钥空值分别表示不筛选。
- `query`、标题、模型、平台秘钥、状态、时间范围全部是 AND 关系；分页总数、列表项、成功/异常数、Token、费用、缓存率必须来自同一条件集。
- 缓存率定义固定为 `cached_input_tokens / input_tokens`，不是请求命中率，也不包含 `cache_write_input_tokens`。
- 无输入 Token的筛选结果缓存率显示 `-`；不得显示 `0%` 误导为已计算的零命中。
- 平台秘钥下拉只展示当前 API key 列表能解析出的名称/ID；日志中的未知历史 key 仍可通过全局查询查看，但不强行注入下拉选项。

# 数据流与状态迁移

页面打开 → 从 startup snapshot 读取模型/API key占位数据 → 查询最新模型目录和 API key 列表 → 用户分别修改三个控件 → React Query key 包含三个显式字段及状态/时间/分页 → `requestlog/list_with_summary` → RPC 反序列化 `RequestLogListParams` → storage 构造统一 WHERE → 返回列表和汇总（含 input/cached）→ 前端格式化缓存率。

旧的 `searchField/search` 状态迁移为 `titleInput/titleSessionIds`、`modelFilter`、`apiKeyFilter`。标题输入仍防抖；标题 lookup 变化后查询结果更新。清空日志的本地缓存 summary 需要同时把 `inputTokens` 与 `cachedInputTokens` 归零。

# 错误处理与失败模式

- 模型/API key lookup 失败时，筛选控件可显示空选项并保留“全部”；日志主查询错误沿用现有错误/重试行为。
- 标题 lookup 返回空匹配时传哨兵 session ID，结果和汇总均为零，不得把空标题当成未筛选。
- 缓存聚合字段缺失时旧服务响应经 normalize 归零；无输入时展示 `-`，不阻塞日志列表。
- 旧 service/Web RPC 仍可反序列化缺失新字段；前端不依赖服务端预计算百分比。

# 安全与隐私

平台秘钥下拉显示已有 `ApiKey.name`，无名时显示既有紧凑 ID 标签；不读取或展示 secret 值。显式 key ID 过滤只缩小结果集，不放宽现有角色/owner 访问控制。

# 兼容性与迁移

无需数据库迁移。RPC 使用 serde 默认字段，桌面 transport 和 Web RPC 均透传 camelCase 新字段；服务端只改查询参数和聚合返回结构。所有现有 `query` 调用保持兼容，之后可按调用方迁移，不新增别名或废弃路径。

# 可观测性

不新增日志或指标。通过 RPC 请求参数、返回 summary 和页面卡片即可观察筛选条件及结果；React Query key 必须完整包含三个独立筛选值，避免缓存串结果。

# 备选方案与否决理由

- 继续把三个字段拼接成一个 query 字符串：否决。现有 query parser 只支持单个表达式，拼接会改变语法并可能产生 OR/AND 歧义。
- 只在前端对当前页过滤：否决。分页和右侧统计会错误，无法满足筛选结果统计。
- 仅新增前端缓存率计算：否决。当前列表分页不能代表完整筛选结果，必须由服务端汇总全量筛选结果。
- 让模型下拉从日志动态 distinct：否决。用户已选择平台模型目录，且会增加新的后端查询和历史数据耦合。

# 发布与回滚

按前端与 Rust RPC 同步发布。回滚时先回退前端控件与新字段传递，再回退服务端扩展；数据库无需回滚。若 Web 与桌面版本暂时混用，serde 默认字段保证旧客户端可调用新服务，但新客户端依赖新服务字段传递，发布顺序应为服务端先行、前端后行。

# 未决技术风险

- `StartupSnapshot.apiModels` 的目录可能不包含已从模型管理中隐藏的历史模型；按已确认的“平台模型目录”决策，历史模型不进入下拉，但全局搜索保持可用。
- 已确认 `crates/service/src/rpc_dispatch/requestlog.rs:37-105` 通过 `serde_json::from_value::<RequestLogListParams>` 直接反序列化并把同一 params 传入 list/summary；无需额外手工参数映射。实现时仍需保持 `requestlog/list`、`requestlog/list_with_summary`、`requestlog/summary` 三条路径同步。

# 验收追溯（AC → 设计点）

- AC-001 → 独立控件、显式 RPC 参数、统一 AND WHERE、模型目录/API key 数据源、Playwright 组合筛选参数断言。
- AC-002 → 汇总 SQL 返回 input/cached Token、前端复用 `formatCacheRate`、`SummaryCard` 展示、Rust 汇总测试与 Playwright 缓存率断言。

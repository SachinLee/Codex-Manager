# 阶段一实施报告：Rust 后端子线程标题投影

完成时间：2026-08-21
范围：Rust resolver 父子关系发现与缓存投影；未修改 TypeScript/UI/i18n
状态：**代码完成，等待串行验证**

## 变更文件

```
crates/service/src/requestlog/requestlog_session_titles.rs       +298 -23
crates/service/src/requestlog/requestlog_session_titles_tests.rs +115 -0
```

## 实施内容

### 1. 数据模型扩展

**`RequestLogSessionTitle`**（L37-46）：
- 新增 `parent_session_id: Option<String>`
- 新增 `parent_title: Option<String>`
- 使用 `#[serde(skip_serializing_if = "Option::is_none")]` 避免主线程序列化冗余字段

**`ExternalSessionTitleCandidate`**（L49-54）：
- 新增 `cache_path: PathBuf`
- 新增 `parent_cache_path: Option<PathBuf>`
- 用于缓存失效判断与父关系重投影

**`OmpSessionFile`**（L85-93）：
- 新增 `parent_cache_path: Option<PathBuf>`
- 新增 `is_child: bool`
- Windows 平台新增 `name: OsString` 用于 derived 目录匹配

### 2. 有界子会话发现

**OMP 布局支持**（L447-573，L802-1046）：
- `<main-session-stem>/<direct-child>.jsonl`
- Unix/Windows 双路径：通过主会话文件 stem 匹配派生目录
- 新增 `MAX_OMP_DERIVED_DIRECTORIES = 512` 上限
- 派生目录消费共享 `MAX_OMP_DIRECTORY_ENTRIES` 预算
- 明确跳过项目级 `subagent-artifacts/` 目录

**Pi 布局支持**：
- `<main-session-stem>/<agent-id>/run-<decimal>/session.jsonl`
- 通过主文件 stem + agent UUID + run 目录三级关联
- 项目级 `subagent-artifacts/*.jsonl` 明确跳过，不猜测归属

**安全边界保留**：
- 所有新增目录层级继续通过 parent handle 相对打开（Windows `open_windows_relative_directory_no_follow`）
- 保留 `FILE_OPEN_REPARSE_POINT` 与 `metadata_is_unsafe_link_or_reparse` 防护
- 保留 `is_single_normal_path_component` 校验，拒绝 `.` / `..` / 路径分隔符

### 3. 子线程元数据读取器

**`read_omp_child_session_title`**（L620-656）：
- 只读取 title slot + session header 两行
- 返回 `title: None`，保留 `session_id` 与 `cwd`
- 设置 `parent_cache_path` 用于后续父关系投影
- 不读取 transcript 后续内容

**`read_pi_child_session_title`**（L713-746）：
- 只读取首行 session header
- 不扫描 `session_info` 或 user message
- 子线程自身 `session_info.name` 不展示，避免暴露子任务 prompt

### 4. 父关系缓存投影

**`project_external_session_title`**（L350-367）：
- 在响应时 join：从 `parent_cache_path` 查找同批次父标题
- 校验同源（`source` 匹配）且非自引用（ID 不同）
- 校验父标题非空，失败时返回 `None`，局部跳过该 child
- 投影后填充 `parent_session_id` 与 `parent_title`

**缓存刷新行为**（L249-322）：
- 父标题改名后，下一次 5 秒 cache refresh 自动更新 child 的 `parent_title`
- 主文件删除或 child 文件删除，缓存剪枝自动移除对应条目
- 不预先固化父标题到 child 缓存，避免改名滞后

### 5. 同源冲突规则

**`merge_request_log_session_titles`**（L145-157）：
- Codex 主线程优先（既有行为）
- OMP/Pi 内部：主线程优先于子线程（`existing.2 == 0 && existing.0.parent_session_id.is_some()`）
- 避免子线程 ID 覆盖主线程条目

### 6. 回归测试

**`omp_subagent_session_uses_parent_title_and_child_id`**（L364-386）：
- 验证 OMP 子线程保留 child ID，`parent_session_id` / `parent_title` 有效，`title` 为 None

**`pi_agent_run_subagent_uses_parent_title_without_reading_child_transcript`**（L388-411）：
- 验证 Pi agent-run 子线程父关系，且不读取子线程 transcript 正文

**`pi_project_subagent_artifacts_are_not_assigned_to_a_parent`**（L413-426）：
- 验证项目级 `subagent-artifacts/*.jsonl` 不被索引

**`omp_subagent_parent_title_refreshes_and_prunes_with_cache`**（L428-461）：
- 验证父标题改名后 child 标题更新
- 验证 child 文件删除后缓存剪枝

**`session_title_merge_prefers_codex_on_id_collision_and_enforces_limit`** 更新（L466-499）：
- 补齐新增字段 `parent_session_id` / `parent_title` / `cache_path` / `parent_cache_path`

## 未修改部分

- 未修改 `request_logs` 数据库 schema
- 未修改网关、请求头解析、session ID 优先级、HTTP/WebSocket/Aggregate finalizer
- 未修改 Codex/OMP/Pi 文件格式或协议
- 未读取或展示子线程 transcript、prompt、工具调用
- 未修改 TypeScript types、normalizer、UI、search、i18n（留待阶段二）

## 串行验证步骤

**用户必须按以下顺序手动执行，不要并行：**

```powershell
# 1. RED: 验证新测试在当前代码下失败（已跳过，代码已实施）

# 2. GREEN: Rust resolver 单元测试
cargo test -p codexmanager-service --lib requestlog_session_titles -- --test-threads=1

# 3. 全 workspace 回归（串行）
cargo test --workspace -- --test-threads=1

# 4. Clippy lint
cargo clippy --workspace --all-targets

# 5. Format check
cargo fmt --check

# 6. 若存在 pre-commit hook
git add crates/service/src/requestlog/
git commit --dry-run
```

## 已知风险

1. **外部布局漂移**：OMP/Pi 未来可能改变目录结构；设计已明确只支持当前已验证布局。
2. **Pi agent-run 头格式差异**：某些运行文件可能不以 `type=session` 开头；本轮会安全跳过。
3. **Windows 竞态与 reparse**：每层继续通过 parent handle 相对打开；已保留现有防护。
4. **远程/容器场景**：service 无法访问客户端本地会话目录时，保持既有"未匹配会话"降级。

## 后续阶段二范围

- TypeScript `RequestLogSessionTitle` 类型扩展
- `normalizeRequestLogSessionTitles` 父字段映射与自引用/半关系校验
- `SessionInfoCell` Badge 与 Tooltip
- 搜索语义 `parentTitle || title`
- i18n 消息同步（`zh-CN` / `en` / `ko` / `ru`）
- 前端 runtime 测试与 Playwright 验证

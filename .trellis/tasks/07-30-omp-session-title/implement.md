# Implementation Plan: Repair OMP Nested Session Discovery

## Preconditions

- 本计划修正已实现功能的发现逻辑；实施前须获得本轮规划摘要的明确批准。
- 只改 service read model 与其回归测试；不得顺带重构 RPC、前端、请求日志持久化或 OMP。
- 遍历深度固定为 sessions root + 一层项目目录，不采用递归 walker。

## Ordered work

1. **先写失效回归测试**
   - 在 `crates/service/src/requestlog/requestlog_session_titles_tests.rs` 增加 `root/<project>/...jsonl` fixture。
   - 断言 `list_omp_session_titles_from_root` 返回真实 UUID/title/cwd；该测试应在当前 direct-only 实现上失败。
   - 增加 root direct 文件兼容、多个子目录/limit、子目录缓存更新与目录/链接降级测试，沿用现有 metadata-only fixture。

2. **安全扩展一层发现**
   - 在 `crates/service/src/requestlog/requestlog_session_titles.rs` 将 flat `collect_omp_session_paths` 改为有界的 root + child-directory 枚举。
   - 重构 `OmpSessionFile`/owner-directory 生命周期，使每个发现的文件保留其已验证父目录。
   - Windows：安全打开每个直接子目录并通过该句柄相对打开 basename；Unix：为子目录保持相同 no-follow 约束。拒绝 symlink/reparse point，计入统一目录项与文件上限。
   - 不改 `read_omp_session_title` 的“两行元数据”边界、ID/title normalization 或 Codex merge precedence。

3. **保持缓存和调用契约**
   - 令 cache 遍历新的完整文件集合；下一次 5 秒刷新必须加入新增子目录文件、删除已消失文件，并复用未变更 entries。
   - 保持 `requestlog/sessionTitles`、Tauri/Web 映射、typed wrapper、搜索和 UI 不变；本轮无前端代码改动。

4. **验证**
   - 运行 `cargo test -p codexmanager-service --lib omp_title`，覆盖 parser、缓存和新增嵌套布局场景。
   - 运行 `cargo test -p codexmanager-service --lib`，验证隔离的 service 包。
   - 以本机 `~/.omp/agent/sessions/abs-Codex-Manager-…/2026-08-04T01-09-37-750Z_019fca51-ab55-7000-beca-006a4140fdfa.jsonl` 为 smoke case：调用运行中服务的 `requestlog/sessionTitles`，确认返回该 ID 与 title；刷新请求日志页，确认“未匹配会话”变为标题。
   - 若交付桌面应用，重建包含 Rust service 的 Tauri bundle，重启后用一条新的 OMP 请求重复上述 UI 验证。服务/容器/远程模式仅在 OMP home 同机可读时纳入此 smoke case。

## Risk gates

- 禁止无限递归或把子路径传入当前 Windows 单组件文件打开 API。
- 禁止在读取失败时回退解析 transcript、首条 prompt 或写入标题快照。
- 禁止改变 `session_id` 获取/持久化链路；截图 UUID 已证明它不是本轮故障。
- 不将 OMP 项加入 `codexSession/*`，避免可变会话管理 API 读取 OMP 数据。

## Rollback point

一层发现代码与测试独立于 RPC/UI/数据库。若需回退，只撤销子目录发现分支，direct-root 兼容与现有会话标题服务不受影响。

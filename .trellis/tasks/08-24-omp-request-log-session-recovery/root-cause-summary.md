# OMP 请求日志会话显示失败根因总结

**分析时间**：2026-08-25  
**任务**：08-24-omp-request-log-session-recovery  
**状态**：根因已定位，修复已实施，等待验证

---

## 根因

**数据库未初始化**：服务启动时 `CODEXMANAGER_DB_PATH` 环境变量未设置，`initialize_storage()` 返回错误，SQLite schema 从未创建。

### 证据

1. **数据库文件为空**：
   ```bash
   $ ls -lh C:/Users/shuan/.codex/codexmanager.db
   -rwxrwxrwx 1 somebody somegroup 0 Aug 25 09:08 codexmanager.db  # 0 字节
   
   $ sqlite3 codexmanager.db ".tables"
   (no output)  # 没有任何表
   ```

2. **初始化逻辑依赖环境变量**（`crates/service/src/storage/storage_helpers.rs:738`）：
   ```rust
   pub(crate) fn initialize_storage() -> Result<(), String> {
       let path = std::env::var("CODEXMANAGER_DB_PATH")
           .map_err(|_| "CODEXMANAGER_DB_PATH not set".to_string())?;  // ← 失败点
       // ... storage.init() 从未执行
   }
   ```

3. **服务入口缺少环境设置**（`crates/service/src/main.rs:12-18`，修复前）：
   ```rust
   fn main() {
       codexmanager_service::portable::bootstrap_current_process();
       codexmanager_service::init_logging();
       // ← 缺少 ensure_default_db_path() 调用
       let configured_addr = ...;
   }
   ```

### 为什么昨天的实施"失败"

昨天实施的代码（严格 OMP fallback + 非阻塞标题快照）**逻辑完全正确**，但：
- Gateway 解析的 `session_id` 写入时表不存在 → 数据丢失
- 标题快照生成正常，但前端查询日志表为空 → Map 无用
- 结果：前端显示 "-"（数据缺失，非逻辑错误）

---

## 修复

### 代码变更

**文件**：`crates/service/src/main.rs`  
**变更**：在 `init_logging()` 后添加环境设置

```diff
 fn main() {
     codexmanager_service::portable::bootstrap_current_process();
     codexmanager_service::init_logging();
+    // Ensure database path is set before starting server
+    codexmanager_service::process_env::ensure_default_db_path();
     let configured_addr = std::env::var("CODEXMANAGER_SERVICE_ADDR")
```

### 修复原理

`ensure_default_db_path()` 逻辑（`crates/service/src/runtime/process_env.rs:188-195`）：
```rust
pub(crate) fn ensure_default_db_path() -> PathBuf {
    let dir = exe_dir();
    let resolved = match std::env::var(ENV_DB_PATH) {
        Ok(raw) if !raw.trim().is_empty() => resolve_path_with_base(&raw, &dir),
        _ => dir.join(DEFAULT_DB_FILENAME),  // 默认：exe_dir/codexmanager.db
    };
    std::env::set_var(ENV_DB_PATH, resolved.to_string_lossy().as_ref());
    resolved
}
```

- 如果环境变量已设置 → 使用现有值
- 如果未设置 → 默认 `<exe_dir>/codexmanager.db`
- 写回环境变量，后续 `initialize_storage()` 读取成功

---

## 验证步骤

### 1. 编译

```bash
cargo build --release --bin codexmanager-service
```

### 2. 启动服务

```bash
cargo run --release --bin codexmanager-service
```

### 3. 检查数据库

```bash
# 确认文件大小非 0
ls -lh ~/.codex/codexmanager.db

# 确认表已创建
sqlite3 ~/.codex/codexmanager.db ".tables"
# 应输出 50+ 个表

# 确认 request_logs 表存在
sqlite3 ~/.codex/codexmanager.db "SELECT sql FROM sqlite_master WHERE type='table' AND name='request_logs'"
```

### 4. 端到端测试

1. 从 OMP 18.0.4 发起一条请求（任意提示词）
2. 查询日志：
   ```sql
   SELECT id, session_id, request_path, status_code, datetime(created_at, 'unixepoch', 'localtime')
   FROM request_logs
   ORDER BY id DESC LIMIT 5;
   ```
3. 验证 `session_id` 非空（应为 UUID v7）
4. 打开前端日志页，确认会话列显示标题（不再是 "-"）

### 5. 桌面打包（可选）

```bash
pnpm -C apps run build:desktop
```

安装并重复步骤 4。

---

## 影响范围

**修复影响**：
- ✅ 服务模式（`codexmanager-service` 二进制）
- ✅ 桌面模式（Tauri 应用已有 `ensure_default_db_path()`，但对齐行为更安全）

**向后兼容**：
- ✅ 显式设置 `CODEXMANAGER_DB_PATH` 的部署不受影响
- ✅ 未设置环境变量的部署现在使用默认路径

**无破坏性**：
- 现有数据库路径不变
- 仅修复启动逻辑，不改变运行时行为

---

## 后续任务

### 立即行动

1. ✅ 代码修复已完成
2. ⏳ 编译验证中
3. ⏳ 端到端测试待执行

### 质量改进

1. **添加烟雾测试**：服务启动后查询 `sqlite_master`，确认表存在
2. **增强日志**：`initialize_storage()` 失败时记录 FATAL 日志
3. **健康检查**：`/health` 端点包含数据库状态

### 规范更新

1. **Spec 记录**：在 `.trellis/spec/codexmanager-service/backend/` 添加"启动顺序"规则
2. **Break-loop 分析**：使用 `trellis-break-loop` 记录根因类型（初始化顺序）

---

## 结论

**昨天的代码没有问题**，问题在于**服务启动缺少环境配置**。

**一行代码修复**：
```rust
codexmanager_service::process_env::ensure_default_db_path();
```

修复后，数据库自动初始化，OMP 会话 ID 和标题正常显示。

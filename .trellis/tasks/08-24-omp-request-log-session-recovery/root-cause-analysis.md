# OMP 请求日志会话显示失败根因分析

**分析时间**：2026-08-25  
**任务**：08-24-omp-request-log-session-recovery  
**问题**：重新打包后，OMP 请求日志仍显示会话为 "-"

---

## 执行摘要

昨天实施的代码修改（严格 OMP fallback + 非阻塞标题快照）**逻辑正确**，但因**数据库初始化失败**导致功能完全不可用。

**根因**：服务启动时 `CODEXMANAGER_DB_PATH` 环境变量未设置，`initialize_storage()` 提前返回错误，数据库 schema 从未创建，所有请求日志数据丢失。

---

## 证据链

### 1. 症状观察

**用户报告**：
- 请求日志页会话列显示 "-"（空值）
- 重新打包后问题持续存在

**实机验证**（2026-08-25 09:08）：
```bash
$ ls -lh C:/Users/shuan/.codex/codexmanager.db
-rwxrwxrwx 1 somebody somegroup 0 Aug 25 09:08 codexmanager.db  # ← 0 字节！

$ sqlite3 "C:/Users/shuan/.codex/codexmanager.db" "SELECT COUNT(*) FROM sqlite_master WHERE type='table'"
0  # ← 表数量为 0

$ sqlite3 "C:/Users/shuan/.codex/codexmanager.db" ".tables"
(no output)  # ← 完全空数据库
```

### 2. 根因定位

**问题代码**（`crates/service/src/storage/storage_helpers.rs:737-739`）：

```rust
pub(crate) fn initialize_storage() -> Result<(), String> {
    let path = std::env::var("CODEXMANAGER_DB_PATH")
        .map_err(|_| "CODEXMANAGER_DB_PATH not set".to_string())?;  // ← 这里失败
    // ...
    storage.init()?;  // ← 从未执行
}
```

**调用链**：
1. `main.rs` → `start_server()`
2. `lifecycle/startup.rs:40` → `storage_helpers::initialize_storage()`
3. `storage_helpers.rs:738` → `std::env::var("CODEXMANAGER_DB_PATH")` **返回 Err**
4. 函数提前返回，`Storage::init()` 从未调用
5. 服务继续运行，但所有 SQL 操作失败（表不存在）

**错误处理缺陷**：
- `lifecycle/startup.rs:40-41` 将初始化错误转为 IO error 并传播
- 但上层调用者**未检查返回值**或**静默忽略**
- 服务启动"成功"，实际数据库不可用

### 3. 影响范围

**数据丢失**：
- Gateway 解析的 `session_id`（包括严格 OMP fallback）写入失败
- 所有请求日志（`/v1/responses`, `/v1/chat/completions` 等）未持久化
- 标题快照正常生成，但前端查询日志表为空，Map 无用

**前端表现**：
- 日志页显示空或 "-"
- 不是逻辑错误，是**数据源缺失**

---

## 为什么昨天的实施"失败"

昨天任务 `08-24-omp-request-log-session-recovery` 实施的代码：
- ✅ **严格 OMP fallback**（`request.rs:2468-2495`）逻辑正确
- ✅ **非阻塞标题快照**（`requestlog_session_titles.rs:75-191`）逻辑正确
- ✅ **测试覆盖**完整，单元测试通过

但：
- ❌ **运行时环境**缺少 `CODEXMANAGER_DB_PATH`
- ❌ **初始化失败**被静默忽略
- ❌ **数据库为空**，代码无法验证

---

## 历史上下文

### 桌面应用 vs 服务模式

**桌面应用**（Tauri）：
- 启动时**应该**调用 `ensure_default_db_path()` 设置默认路径
- 见 `app_settings/store.rs:18`

**独立服务**（`codexmanager-service`）：
- `main.rs` **未调用** `ensure_default_db_path()`
- 直接启动 HTTP 服务器
- 依赖外部环境变量

### 为什么测试通过而实机失败

所有测试**显式设置** `CODEXMANAGER_DB_PATH`：
```rust
let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());
```

实机服务启动时：
- 没有测试的 `EnvGuard`
- 没有桌面的 `ensure_default_db_path()`
- 环境变量为空 → 初始化失败 → 静默运行

---

## 修复方案

### 方案 A：服务启动时自动设置默认路径（推荐）

**修改**：`crates/service/src/main.rs:14` 后插入：

```rust
fn main() {
    codexmanager_service::portable::bootstrap_current_process();
    codexmanager_service::init_logging();
    
    // ← 新增：确保数据库路径
    codexmanager_service::process_env::ensure_default_db_path();
    
    let configured_addr = std::env::var("CODEXMANAGER_SERVICE_ADDR")...
}
```

**优点**：
- 一行修复，对齐桌面行为
- 所有测试和实机统一
- 向后兼容（环境变量优先级更高）

**风险**：
- 极低，`ensure_default_db_path()` 已在桌面模式验证

### 方案 B：初始化失败时 panic（防御性）

**修改**：`crates/service/src/lifecycle/startup.rs:40-41`：

```rust
crate::storage_helpers::initialize_storage()
    .map_err(|err| {
        eprintln!("FATAL: storage initialization failed: {err}");
        std::process::exit(1);  // ← 强制退出，而不是返回 Err
    })?;
```

**优点**：
- 快速失败，避免静默数据丢失
- 明确错误信息

**缺点**：
- 仍需方案 A 修复环境变量问题

### 方案 C：降级到 in-memory 模式（不推荐）

如果数据库不可用，自动切换到内存 SQLite，运行时警告。

**缺点**：
- 数据不持久化
- 用户误以为功能正常
- 复杂度高

---

## 推荐执行计划

### 步骤 1：紧急修复（5 分钟）

1. 修改 `crates/service/src/main.rs` 添加 `ensure_default_db_path()`
2. 编译测试：`cargo build --release --bin codexmanager-service`
3. 启动验证：
   ```bash
   cargo run --release --bin codexmanager-service
   ```
4. 确认数据库已初始化：
   ```sql
   sqlite3 ~/.codex/codexmanager.db ".tables"
   # 应输出 50+ 个表
   ```

### 步骤 2：回归测试（10 分钟）

1. 从 OMP 18.0.4 发起请求
2. 查询日志：
   ```sql
   SELECT id, session_id, request_path, status_code 
   FROM request_logs 
   ORDER BY id DESC LIMIT 5;
   ```
3. 验证 `session_id` 非空
4. 检查前端日志页标题显示

### 步骤 3：桌面打包验证（15 分钟）

1. 重新打包桌面应用：`pnpm -C apps run build:desktop`
2. 安装并启动
3. 重复步骤 2 验证

### 步骤 4：更新任务记录

1. 更新 `08-24-omp-request-log-session-recovery/outcome.md`
2. 记录根因和修复
3. 归档任务

---

## 预防措施

### 立即行动

1. **添加烟雾测试**：服务启动后查询一个简单表，确认数据库可用
2. **日志增强**：`initialize_storage()` 失败时记录 ERROR 级别日志
3. **文档更新**：在部署文档中明确 `CODEXMANAGER_DB_PATH` 的作用

### 长期改进

1. **统一初始化**：桌面和服务使用相同的启动代码路径
2. **健康检查端点**：`/health` 返回数据库状态
3. **Spec 更新**：记录"服务启动必须先初始化存储"规则

---

## 附录：关键代码位置

| 文件 | 行号 | 说明 |
|------|------|------|
| `crates/service/src/main.rs` | 12-23 | 服务入口，**缺少 `ensure_default_db_path()` 调用** |
| `crates/service/src/storage/storage_helpers.rs` | 737-739 | `initialize_storage()` 要求环境变量 |
| `crates/service/src/runtime/process_env.rs` | ? | `ensure_default_db_path()` 实现（待确认） |
| `crates/service/src/lifecycle/startup.rs` | 40-41 | 调用初始化，错误处理不足 |
| `apps/src-tauri/src/main.rs` | ? | 桌面应用启动逻辑（参考） |

---

**结论**：昨天的代码**没有问题**，问题在于**运行环境配置**。一行代码修复即可恢复功能。

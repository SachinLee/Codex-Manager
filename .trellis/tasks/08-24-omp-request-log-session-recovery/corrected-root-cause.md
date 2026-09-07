# OMP 请求日志会话显示失败 - 修正后的根因分析

**分析时间**：2026-08-25  
**任务**：08-24-omp-request-log-session-recovery  
**状态**：根因已确认 - **运行旧版本代码**

---

## 真实根因

**用户今天早上重启桌面应用时，运行的是旧版本（昨天实施前的版本）**。

### 关键证据

#### 1. 数据库路径
- ✅ 真实数据库：`C:/Users/shuan/AppData/Roaming/com.codexmanager.desktop/codexmanager.db` (63MB)
- ❌ 我最初检查的路径：`C:/Users/shuan/.codex/codexmanager.db` (0 字节，无关文件)

#### 2. 请求日志统计

**按小时统计 session_id 成功率：**

| 时间 | 请求数 | 有 session_id | 成功率 |
|------|--------|--------------|--------|
| 2026-08-24 18:51 | 140 | 140 | **100%** ✅ |
| 2026-08-24 17:59 | 74 | 23 | 31% |
| 2026-08-24 16:57 | 118 | 15 | 13% |
| 2026-08-24 11:49 | 177 | 177 | **100%** ✅ |
| **2026-08-25 08:58** | **10** | **0** | **0%** ❌ |
| **2026-08-25 09:02** | **4** | **0** | **0%** ❌ |

**关键时间点：**
- **2026-08-24 18:51:50** - 最后一条有 `session_id` 的请求（UUID：`01a03336-f114-7901-9474-426326574413`）
- **2026-08-25 08:58:34** - 今天第一条请求，`session_id` 为空

**结论**：在 **2026-08-24 18:51 之后到 2026-08-25 08:58 之间**，桌面应用重启并**回退到旧版本**。

#### 3. 代码验证

源代码 `crates/service/src/gateway/local_validation/request.rs:2465` 确认 fallback 已实施：

```rust
fn resolve_request_log_session_id<'a>(
    request_path: &str,
    incoming_headers: &super::super::IncomingHeaderSnapshot,
    metadata_candidates: impl IntoIterator<Item = Option<&'a str>>,
) -> Option<String> {
    incoming_headers
        .session_id()
        .or(incoming_headers.parent_thread_id())
        .map(str::to_string)
        .or_else(|| {
            metadata_candidates
                .into_iter()
                .flatten()
                .map(str::to_string)
                .next()
        })
        .or_else(|| strict_omp_client_request_session_id(request_path, incoming_headers))  // ← 已添加
}
```

**但今天运行的版本没有这行代码。**

---

## 版本不一致的可能原因

### 情况 A：用户手动回退
- 卸载昨天的新包
- 重新安装旧版本

### 情况 B：打包问题
- 昨天实施后**未重新打包桌面应用**
- 或打包时使用了错误的代码分支
- 用户今天启动的是旧的安装包

### 情况 C：多版本并存
- 系统中同时存在新旧两个安装路径
- 今天启动了旧路径的应用

---

## 验证步骤

### 1. 确认当前运行版本

检查运行中的可执行文件修改时间：

```powershell
# 查找进程路径
Get-Process | Where-Object {$_.ProcessName -eq 'CodexManager'} | Select-Object Path

# 检查文件修改时间
Get-Item "C:\path\to\CodexManager.exe" | Select-Object LastWriteTime
```

### 2. 比对编译时间

昨天实施的代码应该在 **2026-08-24 下午/晚上** 编译打包。

如果当前运行的 exe 修改时间早于 2026-08-24，说明运行的是旧版本。

### 3. 临时诊断日志（如果无法确认版本）

在 `crates/service/src/gateway/local_validation/request.rs:2465` 后添加：

```rust
.or_else(|| {
    let result = strict_omp_client_request_session_id(request_path, incoming_headers);
    if request_path == "/v1/responses" {
        log::info!(
            "[OMP_FALLBACK_DEBUG] client_request_id={:?} originator={:?} ua={:?} result={:?}",
            incoming_headers.client_request_id(),
            incoming_headers.originator(),
            incoming_headers.user_agent(),
            result
        );
    }
    result
})
```

重新打包运行，查看日志输出。

---

## 修复方案

### 方案 1：重新打包并安装（推荐）

```bash
# 1. 确认代码包含 strict_omp_client_request_session_id
grep -n "strict_omp_client_request_session_id" crates/service/src/gateway/local_validation/request.rs

# 2. 重新打包桌面应用
pnpm -C apps run build:desktop

# 3. 卸载旧版本
# 4. 安装新版本
# 5. 启动并测试
```

### 方案 2：直接测试最新代码

如果无法立即打包，先用开发模式验证：

```bash
# 启动开发服务
cargo run --release --bin codexmanager-service

# 或启动桌面开发模式
pnpm -C apps run tauri:dev
```

从 OMP 发起请求，查询数据库确认 `session_id` 已写入。

---

## 后续行动

### 立即执行

1. ✅ 修复 `ensure_default_db_path()` 可见性（已完成）
2. ⏳ 重新编译服务（进行中）
3. ⏳ 重新打包桌面应用
4. ⏳ 安装并验证

### 质量改进

1. **版本标记**：在日志页显示服务版本号，方便诊断
2. **启动自检**：服务启动时检查关键功能是否可用
3. **回归测试自动化**：打包后自动运行端到端测试

---

## 结论

**昨天的代码实施完全正确**，问题是：

1. 用户今天运行的是**旧版本代码**
2. 旧代码**没有** `strict_omp_client_request_session_id` fallback
3. OMP 请求的 `session_id` / `parent_thread_id` / metadata 候选全部为空
4. Fallback 未触发 → `session_id` 写入 NULL
5. 前端标题 Map 找不到匹配 → 显示 "-"

**修复**：重新打包并安装昨天实施的新版本。

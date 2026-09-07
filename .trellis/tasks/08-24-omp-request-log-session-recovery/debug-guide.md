# OMP 请求日志调试指南

## 当前状态

✅ 已添加调试日志到 `resolve_request_log_session_id`  
✅ Rust 编译成功  
⏳ 需要重新打包桌面应用并查看日志

---

## 步骤 1：重新打包

```bash
pnpm -C apps run build:desktop
```

**预计时间**：10-15 分钟

---

## 步骤 2：安装新版本

1. 关闭当前 CodexManager
2. 安装新打包的 `.msi` 或 `.exe`
3. 启动应用

---

## 步骤 3：查看服务日志

### 3.1 找到日志文件

日志文件通常在：
- `C:/Users/shuan/AppData/Roaming/com.codexmanager.desktop/logs/`
- 或 `C:/Users/shuan/.codex/logs/`

查找最新的 `.log` 文件

### 3.2 从 OMP 发起请求

随便发一条提示词

### 3.3 查看日志输出

打开最新的日志文件，搜索 `[OMP_DEBUG]`，你应该看到类似：

```
[OMP_DEBUG] client_req_id=Some("01a03666-3c77-7000-...") originator=Some("pi") ua=Some("omp/18.0.4") session_id_result=Some("01a03666-3c77-7000-...")
```

或者：

```
[OMP_DEBUG] client_req_id=None originator=None ua=None session_id_result=None
```

---

## 步骤 4：根据日志结果诊断

### 情况 A：`client_req_id=None`

**说明**：OMP 没有发送 `x-client-request-id` header

**可能原因**：
1. OMP 版本不对（不是 18.0.4）
2. OMP 配置问题
3. 中间层（代理/反代）删除了 header

**解决**：
- 确认 OMP 版本
- 检查 OMP 配置是否禁用了某些 header

### 情况 B：`originator=None`

**说明**：OMP 没有发送 `originator: pi` header

**可能原因**：
1. OMP 18.0.4 实际不发送这个 header
2. 昨天的研究文档基于旧版本（17.1.8）

**解决**：
- 放宽 `strict_omp_client_request_session_id` 的条件
- 移除 `originator` 检查

### 情况 C：`ua=None` 或 `ua=Some("other")`

**说明**：User-Agent 不匹配 `omp/...` 格式

**可能原因**：
1. OMP 18.0.4 的 UA 格式不同
2. 中间层修改了 UA

**解决**：
- 放宽 UA 检查
- 或完全移除 UA 条件

### 情况 D：所有值都有，但 `session_id_result=None`

**说明**：`is_canonical_uuid_v7` 检查失败

**可能原因**：
- `client_request_id` 不是 UUIDv7 格式
- 或格式验证太严格

**解决**：
- 放宽 UUID 验证
- 或直接使用 `client_request_id` 而不验证格式

---

## 步骤 5：根据实际情况调整代码

基于日志输出，我们可以：

1. **如果 `originator` 永远为 None**：移除该条件
2. **如果 `ua` 格式不对**：调整格式匹配
3. **如果 UUID 不是 v7**：放宽验证或移除验证

---

## 快速诊断命令

### 查看最新日志

```powershell
# PowerShell
Get-Content "C:/Users/shuan/AppData/Roaming/com.codexmanager.desktop/logs/*.log" -Tail 100 | Select-String "OMP_DEBUG"
```

### 查看数据库

```bash
sqlite3 "C:/Users/shuan/AppData/Roaming/com.codexmanager.desktop/codexmanager.db" \
  "SELECT id, session_id, datetime(created_at, 'unixepoch', 'localtime') 
   FROM request_logs 
   WHERE request_path = '/v1/responses' 
   ORDER BY id DESC LIMIT 3"
```

---

## 预期结果

如果一切正常，日志应显示：
```
[OMP_DEBUG] client_req_id=Some("01a03...") originator=Some("pi") ua=Some("omp/18.0.4") session_id_result=Some("01a03...")
```

并且数据库最新行的 `session_id` 应为该 UUID。

---

## 如果还是不行

将日志中的 `[OMP_DEBUG]` 行发给我，我可以：
1. 确认实际的 header 值
2. 调整 fallback 条件
3. 重新编译并打包

---

**当前时间**：2026-08-25 09:30  
**下一步**：重新打包 → 安装 → 测试 → 查看日志

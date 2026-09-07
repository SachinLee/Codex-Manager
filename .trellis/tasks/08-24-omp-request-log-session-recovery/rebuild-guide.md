# 桌面应用重新打包步骤

## 前提条件

1. ✅ Rust 代码已修改（`strict_omp_client_request_session_id` fallback）
2. ⏳ Rust 服务正在编译
3. 🎯 目标：重新打包桌面应用，包含最新代码

---

## 步骤 1：确认 Rust 编译完成

```bash
# 检查编译状态
cargo build --release --bin codexmanager-service

# 如果成功，应看到：
#   Finished `release` profile [optimized] target(s) in X.XXs
```

**预期时间**：5-10 分钟（取决于机器性能）

---

## 步骤 2：打包桌面应用

### 2.1 清理旧构建产物（可选但推荐）

```bash
cd apps
pnpm run clean  # 如果有该命令
# 或手动删除
rm -rf .next out dist
```

### 2.2 执行桌面打包

```bash
# 从项目根目录执行
pnpm -C apps run build:desktop
```

**这个命令会：**
1. 编译 Next.js 前端（静态导出）
2. 编译 Tauri Rust 后端
3. 打包成桌面安装程序

**预期时间**：10-15 分钟

**输出位置**：
- Windows: `apps/src-tauri/target/release/bundle/msi/` 或 `apps/src-tauri/target/release/bundle/nsis/`
- 安装包名称类似：`CodexManager_x.x.x_x64_en-US.msi` 或 `.exe`

---

## 步骤 3：安装新版本

### 3.1 关闭当前运行的应用

```powershell
# 停止 CodexManager 进程
Stop-Process -Name "CodexManager" -Force
```

或手动：右键托盘图标 → 退出

### 3.2 卸载旧版本（可选）

- Windows 设置 → 应用 → CodexManager → 卸载
- 或直接覆盖安装（有些安装程序支持）

### 3.3 安装新打包的版本

运行 `apps/src-tauri/target/release/bundle/msi/CodexManager_xxx.msi`

---

## 步骤 4：验证修复

### 4.1 启动新版本

双击桌面图标或开始菜单启动

### 4.2 从 OMP 发起请求

随便发一条提示词

### 4.3 查询数据库确认

```powershell
sqlite3 "C:/Users/shuan/AppData/Roaming/com.codexmanager.desktop/codexmanager.db" "SELECT id, session_id, request_path, datetime(created_at, 'unixepoch', 'localtime') FROM request_logs WHERE request_path = '/v1/responses' ORDER BY id DESC LIMIT 5"
```

**预期结果**：
- 最新请求的 `session_id` 列应显示 UUID（如 `01a03666-3c77-7000-...`）
- 而不是空值 `||`

### 4.4 查看前端日志页

打开 CodexManager → 请求日志页

**预期结果**：
- 会话列应显示 OMP 会话标题
- 而不是 "-"

---

## 常见问题

### Q1: `pnpm -C apps run build:desktop` 失败

**检查：**
```bash
# 确认 pnpm 已安装
pnpm --version

# 确认依赖已安装
cd apps
pnpm install
```

### Q2: Tauri 打包失败

**可能原因：**
- Rust 工具链版本不对
- 缺少 Windows SDK

**解决：**
```bash
# 更新 Rust
rustup update

# 确认 Tauri CLI
cargo install tauri-cli --version 2.0
```

### Q3: 安装新版本后仍显示 "-"

**检查：**
1. 确认进程确实重启了：
   ```powershell
   Get-Process CodexManager | Select-Object Path, StartTime
   ```
2. 确认可执行文件是最新的：
   ```powershell
   Get-Item "C:/Program Files/CodexManager/CodexManager.exe" | Select-Object LastWriteTime
   ```
   应该是今天的日期

3. 查看服务日志，确认 fallback 逻辑执行

---

## 快捷方式（开发模式验证）

如果打包太慢，可以先用开发模式快速验证：

```bash
# 启动开发模式（包含最新代码）
pnpm -C apps run tauri:dev
```

这会启动一个开发版本的应用，功能完全相同，但不需要打包。

验证通过后，再执行正式打包。

---

## 预计总耗时

- Rust 编译：5-10 分钟
- 桌面打包：10-15 分钟
- 安装验证：2-3 分钟

**总计：约 20-30 分钟**

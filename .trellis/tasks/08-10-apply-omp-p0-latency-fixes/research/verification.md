# P0 执行与验证记录

日期：2026-08-10
任务：apply-omp-p0-latency-fixes

## 已应用

| 来源 | 变更 | 理由 |
|---|---|---|
| `C:/Users/shuan/.claude.json` | 删除 `context7` | OMP 已保留 `Context7` 实例；删除同名重复源 |
| `C:/Users/shuan/.claude.json` | 删除 `codex` | 日志持续 `Transport closed`，当前 OMP 不使用 |
| `C:/Users/shuan/.claude.json` | 删除 `ida-pro-mcp` | 日志持续连接本机 IDA 失败，当前 OMP 不使用 |
| `C:/Users/shuan/.codex/config.toml` | 删除 `[mcp_servers.openaiDeveloperDocs]` | 日志持续 HTTP 403，当前 OMP 不使用 |
| `C:/Users/shuan/.claude/plugins/installed_plugins.json` | 删除失效 Postman 项目插件注册 | 插件无认证且已禁用；残留注册仍触发 OMP 加载 |
| `C:/Users/shuan/.claude/settings.json` | `postman@claude-plugins-official: false` | 保留可逆禁用状态，不删除插件缓存 |
| `C:/Users/shuan/.omp/agent/config.yml` | `modelRoles.tiny: codex-manager/gpt-5.6-terra:low` | 标题请求从反复失败的 DeepSeek Flash 切换到已验证可响应的 Terra 低推理档；不改变 default 主模型 |

## 回滚

- 任务备份：`research/config-backup.md`，保存所有被删除的 MCP 完整配置段和旧值。
- Claude 完整自动备份：`C:/Users/shuan/.claude/backups/.claude.json.backup.1786073360926`。
- 恢复后重启 OMP；标题角色恢复命令见备份文件。

## 验证证据

- `omp config get modelRoles --json` 成功，返回 `tiny = codex-manager/gpt-5.6-terra:low`。
- `claude plugin list` 成功解析 Claude 配置；Postman 不再出现在已安装插件列表，Codex 插件状态为 disabled。
- `C:/Users/shuan/.omp/logs/omp.2026-08-10.36032.log`：标题请求使用 Terra，约 2.2 秒完成并记录 `title-generator: success`；此前 DeepSeek 失败样本为 19.9–72.3 秒且同一会话重试四次。
- `omp` 冷启动 smoke 返回 `OMP_P0_OK`，证明主会话仍能正常响应。

## 验证限制

本会话由 OMP 工具桥启动，子进程继承了 `PI_TOOL_BRIDGE_*` 会话桥。该桥会额外注入当前宿主的 MCP 清单，因此嵌套 smoke 输出仍显示 `postman`、`ida-pro-mcp`、`codex` 等失败项；这不是从已修改的三个权威配置文件重新读取出的清单。已通过环境变量检查确认该污染来源。需在独立 Windows Terminal 中启动一个全新 OMP 进程，才能对“无失效 MCP 冷启动”做无桥接 A/B 对照；本任务没有把嵌套桥接结果冒充为通过。

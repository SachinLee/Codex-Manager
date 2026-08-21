# OMP P0 修改前可恢复备份
# 创建时间：2026-08-10
# 仅保存本任务将修改的键及完整配置段；按原路径和内容恢复即可。

C:/Users/shuan/.omp/agent/config.yml
providers.tinyModel: online

modelRoles.tiny: aiswitch-china/deepseek-v4-flash-0731:high
C:/Users/shuan/.claude/settings.json
enabledPlugins.postman@claude-plugins-official: true

C:/Users/shuan/.claude.json
mcpServers.context7:
{
  "type": "stdio",
  "command": "cmd",
  "args": ["/c", "npx", "-y", "@upstash/context7-mcp@2.1.4"]
}

mcpServers.codex:
{
  "args": ["--from", "git+https://github.com/GuDaStudio/codexmcp.git", "codexmcp"],
  "command": "C:\\Users\\shuan\\AppData\\Local\\Microsoft\\WinGet\\Packages\\astral-sh.uv_Microsoft.Winget.Source_8wekyb3d8bbwe\\uvx.exe"
}

mcpServers.ida-pro-mcp:
{
  "args": [
    "C:\\Users\\shuan\\AppData\\Local\\Python\\pythoncore-3.14-64\\Lib\\site-packages\\ida_pro_mcp\\server.py",
    "--ida-rpc",
    "http://127.0.0.1:13337"
  ],
  "command": "C:\\Users\\shuan\\AppData\\Local\\Python\\pythoncore-3.14-64\\python.exe"
}

C:/Users/shuan/.codex/config.toml
[mcp_servers.openaiDeveloperDocs]
url = "https://developers.openai.com/mcp"

恢复步骤：
1. 将上述 MCP JSON 对象重新添加到相应配置的 mcpServers；将 Postman enabledPlugins 值恢复为 true。
2. 将 [mcp_servers.openaiDeveloperDocs] 段恢复至 C:/Users/shuan/.codex/config.toml。
3. 执行：omp config set providers.tinyModel online
4. 执行：omp config set modelRoles.tiny aiswitch-china/deepseek-v4-flash-0731:high

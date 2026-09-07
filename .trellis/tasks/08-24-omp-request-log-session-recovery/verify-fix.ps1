# 验证数据库初始化修复
# 2026-08-25

Write-Host "=== 验证数据库初始化修复 ===" -ForegroundColor Cyan

# 1. 检查数据库文件
$dbPath = "$env:USERPROFILE\.codex\codexmanager.db"
Write-Host "`n步骤 1: 检查数据库文件" -ForegroundColor Yellow

if (Test-Path $dbPath) {
    $size = (Get-Item $dbPath).Length
    Write-Host "  ✓ 数据库文件存在: $dbPath" -ForegroundColor Green
    Write-Host "  ✓ 文件大小: $size 字节" -ForegroundColor Green
    
    if ($size -eq 0) {
        Write-Host "  ✗ 警告：数据库文件为空！" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "  ✗ 数据库文件不存在" -ForegroundColor Red
    exit 1
}

# 2. 检查表结构
Write-Host "`n步骤 2: 检查表结构" -ForegroundColor Yellow
$tableCount = sqlite3 $dbPath "SELECT COUNT(*) FROM sqlite_master WHERE type='table'"
Write-Host "  ✓ 表数量: $tableCount" -ForegroundColor Green

if ($tableCount -eq 0) {
    Write-Host "  ✗ 错误：没有任何表！" -ForegroundColor Red
    exit 1
}

# 3. 检查 request_logs 表
Write-Host "`n步骤 3: 检查 request_logs 表" -ForegroundColor Yellow
$hasRequestLogs = sqlite3 $dbPath "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='request_logs'"

if ($hasRequestLogs -eq 1) {
    Write-Host "  ✓ request_logs 表存在" -ForegroundColor Green
    
    # 检查表结构
    $columns = sqlite3 $dbPath "PRAGMA table_info(request_logs)" | Select-String -Pattern "session_id"
    if ($columns) {
        Write-Host "  ✓ session_id 列存在" -ForegroundColor Green
    } else {
        Write-Host "  ✗ session_id 列缺失" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "  ✗ request_logs 表不存在" -ForegroundColor Red
    exit 1
}

# 4. 模拟插入测试（可选）
Write-Host "`n步骤 4: 测试写入（跳过，等待真实请求）" -ForegroundColor Yellow

Write-Host "`n=== 数据库初始化验证通过 ===" -ForegroundColor Green
Write-Host "现在可以从 OMP 发起请求进行端到端验证" -ForegroundColor Cyan

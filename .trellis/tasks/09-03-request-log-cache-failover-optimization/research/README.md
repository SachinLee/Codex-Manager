# 缓存率分析研究目录

## 目标

通过实际请求日志数据验证以下假设:
1. 上游账号切换频率与缓存率呈负相关
2. 会话内账号稳定性影响缓存命中率
3. `ordered` vs `balanced` 路由策略对缓存的影响
4. Conversation Binding 是否有效保持账号稳定性

## 执行步骤

### 1. 定位数据库文件

Codex Manager 的请求日志存储在 SQLite 数据库中,默认位置:

**Desktop 模式**:
- Windows: `%LOCALAPPDATA%\com.codexmanager.desktop\codexmanager.db`
- macOS: `~/Library/Application Support/com.codexmanager.desktop/codexmanager.db`
- Linux: `~/.local/share/com.codexmanager.desktop/codexmanager.db`

**Service 模式**:
- 环境变量 `CODEXMANAGER_DB_PATH` 指定的路径
- 或可执行文件同级目录下的 `codexmanager.db`

**检查方法**:
```bash
# 查找当前运行的 Codex Manager 进程
ps aux | grep codexmanager  # Linux/macOS
tasklist | findstr codex    # Windows

# 或检查环境变量
echo $CODEXMANAGER_DB_PATH
```

### 2. 执行 SQL 查询

使用以下任一工具执行 `cache-analysis-queries.sql`:

**方法 A: sqlite3 命令行**
```bash
sqlite3 /path/to/codexmanager.db < cache-analysis-queries.sql > results.txt
```

**方法 B: DB Browser for SQLite (GUI)**
1. 下载: https://sqlitebrowser.org/
2. 打开数据库文件
3. 进入 "Execute SQL" 标签页
4. 复制粘贴查询并逐条执行
5. 导出结果为 CSV

**方法 C: Python 脚本**
```python
import sqlite3
import pandas as pd

conn = sqlite3.connect('/path/to/codexmanager.db')

# 执行 Q1
df = pd.read_sql_query("""
    SELECT ...  -- 从 cache-analysis-queries.sql 复制
""", conn)

print(df)
df.to_csv('q1_result.csv', index=False)
```

### 3. 收集结果

将每个查询的结果保存到 `results/` 目录:
- `q1_overall_cache_rate.csv`: 综合缓存率
- `q2_by_route_strategy.csv`: 按路由策略分组
- `q3_attempt_frequency.csv`: 账号切换频率
- `q4_session_account_stability.csv`: 会话内账号稳定性
- `q5_by_account.csv`: 按账号分组
- `q6_by_model.csv`: 按模型分组
- `q7_failure_modes.csv`: 故障模式
- `q8_hourly_trend.csv`: 时间序列
- `q9_high_retry_samples.csv`: 高频切换样本
- `q10_binding_effectiveness.csv`: Conversation Binding 有效性

### 4. 分析结果

在 `cache-analysis.md` 中记录:
- 实际综合缓存率是否接近用户观测的 66%
- 账号切换次数与缓存率的数值关系
- 路由策略差异的量化证据
- 关键发现与假设验证结论

## 关键指标定义

**Cache Hit Rate (缓存命中率)**:
```
cache_hit_rate = cached_input_tokens / input_tokens * 100%
```

**账号切换频率**:
- 通过 `attempted_account_ids_json` 字段计算尝试次数
- `attempt_count = 1`: 单次成功,无切换
- `attempt_count >= 2`: 发生故障切换

**会话内账号稳定性**:
- 同一 `session_id` 内连续请求使用相同 `account_id` = 稳定
- 账号变更 = 不稳定,理论上会导致缓存重建

## 预期发现

如果假设成立,应观察到:
1. **Q3 结果**: `attempt_count = 1` 的请求缓存率 > `attempt_count >= 2`
2. **Q4 结果**: "账号未切换"的缓存率 >> "账号已切换"
3. **Q2 结果**: `route_source = conversation_binding` 的缓存率较高
4. **Q5 结果**: 某些账号的缓存率显著高于其他账号(说明缓存未跨账号共享)

如果假设不成立(缓存率与切换无关),则需要重新评估优化方向。

## 数据隐私

请求日志包含用户会话元数据,分析结果应:
- 脱敏处理 `account_id`/`session_id`/`trace_id`
- 仅展示聚合统计,不暴露单条请求内容
- 不提交包含真实 ID 的原始日志到版本控制

## 下一步

基于数据分析结果,进入 PRD Requirements 阶段,选择合适的优化策略。

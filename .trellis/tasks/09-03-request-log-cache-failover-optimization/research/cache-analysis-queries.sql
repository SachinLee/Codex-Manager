-- 请求日志缓存率分析查询
-- 目标:验证上游故障切换与缓存率的相关性
-- 执行方式:在 Codex Manager 数据库中依次执行以下查询

-- ============================================
-- Q1. 综合缓存率(最近 24 小时)
-- ============================================
-- 验证用户观测的 66% 缓存率

SELECT 
    COUNT(*) as total_requests,
    SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END) as success_requests,
    SUM(input_tokens) as total_input_tokens,
    SUM(cached_input_tokens) as total_cached_tokens,
    ROUND(100.0 * SUM(cached_input_tokens) / NULLIF(SUM(input_tokens), 0), 2) as cache_hit_rate_pct,
    SUM(output_tokens) as total_output_tokens
FROM request_token_stats
WHERE 
    started_at >= (strftime('%s', 'now') - 86400) * 1000
    AND client_model LIKE 'gpt-%'
    AND input_tokens > 0;

-- ============================================
-- Q2. 按路由策略分组的缓存率
-- ============================================
-- 对比 ordered / balanced / conversation_binding 的缓存表现

SELECT 
    COALESCE(route_strategy, '(null)') as route_strategy,
    COALESCE(route_source, '(null)') as route_source,
    COUNT(*) as request_count,
    SUM(input_tokens) as total_input_tokens,
    SUM(cached_input_tokens) as total_cached_tokens,
    ROUND(100.0 * SUM(cached_input_tokens) / NULLIF(SUM(input_tokens), 0), 2) as cache_hit_rate_pct
FROM request_token_stats
WHERE 
    started_at >= (strftime('%s', 'now') - 86400) * 1000
    AND client_model LIKE 'gpt-%'
    AND input_tokens > 0
GROUP BY route_strategy, route_source
ORDER BY cache_hit_rate_pct DESC;

-- ============================================
-- Q3. 账号切换频率分析
-- ============================================
-- 统计触发多次上游尝试的请求占比

WITH attempt_analysis AS (
    SELECT 
        trace_id,
        account_id,
        initial_account_id,
        attempted_account_ids_json,
        -- 计算尝试次数:JSON 数组长度
        CASE 
            WHEN attempted_account_ids_json IS NULL THEN 1
            WHEN attempted_account_ids_json = '[]' THEN 1
            ELSE (LENGTH(attempted_account_ids_json) - LENGTH(REPLACE(attempted_account_ids_json, ',', '')) + 1)
        END as attempt_count,
        input_tokens,
        cached_input_tokens,
        status_code
    FROM request_token_stats
    WHERE 
        started_at >= (strftime('%s', 'now') - 86400) * 1000
        AND client_model LIKE 'gpt-%'
        AND input_tokens > 0
)
SELECT 
    CASE 
        WHEN attempt_count = 1 THEN '单次成功'
        WHEN attempt_count = 2 THEN '2次尝试'
        WHEN attempt_count = 3 THEN '3次尝试'
        ELSE '4+次尝试'
    END as attempt_category,
    COUNT(*) as request_count,
    ROUND(100.0 * COUNT(*) / SUM(COUNT(*)) OVER (), 2) as request_pct,
    SUM(input_tokens) as total_input_tokens,
    SUM(cached_input_tokens) as total_cached_tokens,
    ROUND(100.0 * SUM(cached_input_tokens) / NULLIF(SUM(input_tokens), 0), 2) as cache_hit_rate_pct
FROM attempt_analysis
GROUP BY attempt_category
ORDER BY attempt_count;

-- ============================================
-- Q4. 会话内账号切换对缓存的影响
-- ============================================
-- 对比同一 session 内账号稳定时 vs 切换后的缓存率

WITH session_requests AS (
    SELECT 
        session_id,
        account_id,
        input_tokens,
        cached_input_tokens,
        started_at,
        ROW_NUMBER() OVER (PARTITION BY session_id ORDER BY started_at) as req_seq,
        LAG(account_id) OVER (PARTITION BY session_id ORDER BY started_at) as prev_account_id
    FROM request_token_stats
    WHERE 
        started_at >= (strftime('%s', 'now') - 86400) * 1000
        AND client_model LIKE 'gpt-%'
        AND session_id IS NOT NULL
        AND session_id != ''
        AND input_tokens > 0
)
SELECT 
    CASE 
        WHEN req_seq = 1 THEN '会话首次请求'
        WHEN account_id = prev_account_id THEN '账号未切换'
        ELSE '账号已切换'
    END as account_stability,
    COUNT(*) as request_count,
    SUM(input_tokens) as total_input_tokens,
    SUM(cached_input_tokens) as total_cached_tokens,
    ROUND(100.0 * SUM(cached_input_tokens) / NULLIF(SUM(input_tokens), 0), 2) as cache_hit_rate_pct
FROM session_requests
GROUP BY account_stability
ORDER BY 
    CASE account_stability
        WHEN '会话首次请求' THEN 1
        WHEN '账号未切换' THEN 2
        ELSE 3
    END;

-- ============================================
-- Q5. 按账号分组的缓存率
-- ============================================
-- 识别哪些账号的缓存率最高/最低

SELECT 
    COALESCE(account_id, '(null)') as account_id,
    COUNT(*) as request_count,
    SUM(input_tokens) as total_input_tokens,
    SUM(cached_input_tokens) as total_cached_tokens,
    ROUND(100.0 * SUM(cached_input_tokens) / NULLIF(SUM(input_tokens), 0), 2) as cache_hit_rate_pct,
    -- 计算该账号被切换到的频率
    SUM(CASE WHEN account_id != initial_account_id THEN 1 ELSE 0 END) as failover_count
FROM request_token_stats
WHERE 
    started_at >= (strftime('%s', 'now') - 86400) * 1000
    AND client_model LIKE 'gpt-%'
    AND input_tokens > 0
GROUP BY account_id
HAVING request_count >= 10  -- 至少 10 次请求才有统计意义
ORDER BY cache_hit_rate_pct DESC;

-- ============================================
-- Q6. 按模型分组的缓存率
-- ============================================
-- 不同模型的缓存表现

SELECT 
    client_model,
    COUNT(*) as request_count,
    SUM(input_tokens) as total_input_tokens,
    SUM(cached_input_tokens) as total_cached_tokens,
    ROUND(100.0 * SUM(cached_input_tokens) / NULLIF(SUM(input_tokens), 0), 2) as cache_hit_rate_pct
FROM request_token_stats
WHERE 
    started_at >= (strftime('%s', 'now') - 86400) * 1000
    AND client_model LIKE 'gpt-%'
    AND input_tokens > 0
GROUP BY client_model
ORDER BY request_count DESC;

-- ============================================
-- Q7. 故障模式识别(需要关联 gateway_policy_actions 或日志)
-- ============================================
-- 如果有 error 字段或关联表,统计触发切换的错误类型

-- 注意:此查询依赖具体的错误日志实现,可能需要调整
SELECT 
    CASE 
        WHEN attempted_account_ids_json IS NOT NULL 
             AND (LENGTH(attempted_account_ids_json) - LENGTH(REPLACE(attempted_account_ids_json, ',', '')) + 1) > 1 
        THEN '多次尝试'
        ELSE '单次成功'
    END as retry_status,
    status_code,
    COUNT(*) as occurrence_count,
    ROUND(100.0 * COUNT(*) / SUM(COUNT(*)) OVER (), 2) as pct
FROM request_token_stats
WHERE 
    started_at >= (strftime('%s', 'now') - 86400) * 1000
    AND client_model LIKE 'gpt-%'
GROUP BY retry_status, status_code
ORDER BY occurrence_count DESC
LIMIT 20;

-- ============================================
-- Q8. 时间序列缓存率(按小时)
-- ============================================
-- 观察缓存率的时间分布,识别是否有特定时段缓存率骤降

SELECT 
    datetime((started_at / 1000 / 3600) * 3600, 'unixepoch') as hour_bucket,
    COUNT(*) as request_count,
    SUM(input_tokens) as total_input_tokens,
    SUM(cached_input_tokens) as total_cached_tokens,
    ROUND(100.0 * SUM(cached_input_tokens) / NULLIF(SUM(input_tokens), 0), 2) as cache_hit_rate_pct
FROM request_token_stats
WHERE 
    started_at >= (strftime('%s', 'now') - 86400) * 1000
    AND client_model LIKE 'gpt-%'
    AND input_tokens > 0
GROUP BY hour_bucket
ORDER BY hour_bucket;

-- ============================================
-- Q9. 高频切换请求样本
-- ============================================
-- 提取需要 3+ 次尝试的请求,用于深入分析

SELECT 
    trace_id,
    session_id,
    client_model,
    initial_account_id,
    account_id as final_account_id,
    attempted_account_ids_json,
    route_strategy,
    route_source,
    status_code,
    input_tokens,
    cached_input_tokens,
    ROUND(100.0 * cached_input_tokens / NULLIF(input_tokens, 0), 2) as cache_hit_rate_pct,
    datetime(started_at / 1000, 'unixepoch') as started_time
FROM request_token_stats
WHERE 
    started_at >= (strftime('%s', 'now') - 86400) * 1000
    AND client_model LIKE 'gpt-%'
    AND input_tokens > 0
    AND attempted_account_ids_json IS NOT NULL
    AND (LENGTH(attempted_account_ids_json) - LENGTH(REPLACE(attempted_account_ids_json, ',', '')) + 1) >= 3
ORDER BY started_at DESC
LIMIT 50;

-- ============================================
-- Q10. Conversation Binding 有效性验证
-- ============================================
-- 检查有会话绑定的请求是否真的保持了账号稳定性

WITH binding_requests AS (
    SELECT 
        r.session_id,
        r.conversation_anchor,
        r.account_id,
        r.input_tokens,
        r.cached_input_tokens,
        r.started_at,
        c.account_id as bound_account_id,
        c.cache_affinity_route_id
    FROM request_token_stats r
    LEFT JOIN conversation_bindings c 
        ON r.session_id = c.local_conversation_id
        OR r.conversation_anchor = c.thread_anchor
    WHERE 
        r.started_at >= (strftime('%s', 'now') - 86400) * 1000
        AND r.client_model LIKE 'gpt-%'
        AND r.input_tokens > 0
        AND r.session_id IS NOT NULL
)
SELECT 
    CASE 
        WHEN bound_account_id IS NULL THEN '无绑定'
        WHEN account_id = bound_account_id THEN '绑定匹配'
        ELSE '绑定不匹配'
    END as binding_status,
    COUNT(*) as request_count,
    SUM(input_tokens) as total_input_tokens,
    SUM(cached_input_tokens) as total_cached_tokens,
    ROUND(100.0 * SUM(cached_input_tokens) / NULLIF(SUM(input_tokens), 0), 2) as cache_hit_rate_pct
FROM binding_requests
GROUP BY binding_status;

from pathlib import Path
import re
p = Path('crates/core/src/storage/request_token_stats.rs')
text = p.read_text(encoding='utf-8')

# 1) add import for GUARD_RETRY_ACTION_SQL
if 'GUARD_RETRY_ACTION_SQL' not in text.split('fn cache_hit_rate')[0][:500] and 'use super::reasoning_guard_events' not in text:
    old = 'use super::key_id_filters::{PairedKeyIdSqlFilter, TempKeyIdFilter};'
    new = old + '\nuse super::reasoning_guard_events::GUARD_RETRY_ACTION_SQL;'
    if old not in text:
        raise SystemExit('import anchor missing')
    text = text.replace(old, new, 1)
    print('added GUARD import')

# 2) add token_total_sql_expr_for after token_total_sql_expr
if 'fn token_total_sql_expr_for' not in text:
    helper = '''
pub(super) fn token_total_sql_expr_for(prefix: &str) -> String {
    format!(
        "CASE
        WHEN {prefix}total_tokens IS NOT NULL THEN
            CASE WHEN {prefix}total_tokens > 0 THEN {prefix}total_tokens ELSE 0 END
        ELSE
            CASE
                WHEN IFNULL({prefix}input_tokens, 0) + IFNULL({prefix}output_tokens, 0) > 0
                    THEN IFNULL({prefix}input_tokens, 0) + IFNULL({prefix}output_tokens, 0)
                ELSE 0
            END
     END"
    )
}

'''
    m = re.search(r'fn token_total_sql_expr\(\) -> &\'static str \{[\s\S]*?\n\}\n', text)
    if not m:
        raise SystemExit('token_total_sql_expr not found')
    text = text[:m.end()] + helper + text[m.end():]
    print('added token_total_sql_expr_for')

# 3) remove duplicate summarize_request_token_stats_by_model injected after account method
# Find second occurrence of "pub fn summarize_request_token_stats_by_model(" after line of by_account
matches = list(re.finditer(r'    pub fn summarize_request_token_stats_by_model\(', text))
print('by_model count', len(matches), 'positions', [m.start() for m in matches])
if len(matches) >= 2:
    # remove from second match until next "pub fn summarize_request_token_stats_by_aggregate_api_between"
    start = matches[1].start()
    next_agg = text.find('    pub fn summarize_request_token_stats_by_aggregate_api_between(', start)
    if next_agg < 0:
        raise SystemExit('aggregate method not found after duplicate')
    text = text[:start] + text[next_agg:]
    print('removed duplicate by_model block')

p.write_text(text, encoding='utf-8')
print('open', text.count('{'), 'close', text.count('}'))
print('done')

from pathlib import Path

ours = Path('.tmp-ours-request_token_stats.rs').read_text(encoding='utf-8').splitlines()
# methods: lines 794-1035 (1-based) => [793:1035]
methods = '\n'.join(ours[793:1035])
# backfill + helpers: from backfill fn through non_blank_str (1927-2071)
backfill_and_helpers = '\n'.join(ours[1926:2071])

main_path = Path('crates/core/src/storage/request_token_stats.rs')
text = main_path.read_text(encoding='utf-8')

# fix imports
old_imp = '''use super::{
    now_ts, ApiKeyModelTokenUsageSummary, ApiKeyTokenUsageSummary, DailyTokenUsageRollup,
    MemberDashboardUsageBreakdownSnapshot, ModelTokenUsageRollup, RequestLogQuerySummary,
    RequestLogTodaySummary, RequestTokenStat, SourceTokenUsageRollup, Storage, TokenUsageRollup,
    TokenUsageSummary, UserTokenUsageRollup,
};'''
new_imp = '''use super::{
    now_ts, AccountDailyUsageSummary, AggregateApiDailyUsageSummary, ApiKeyModelTokenUsageSummary,
    ApiKeyTokenUsageSummary, DailyTokenUsageRollup, MemberDashboardUsageBreakdownSnapshot,
    ModelTokenUsageRollup, RequestLogQuerySummary, RequestLogTodaySummary, RequestTokenStat,
    SourceTokenUsageRollup, Storage, TokenUsageRollup, TokenUsageSummary, UserTokenUsageRollup,
};
use serde_json::Value as JsonValue;'''
if old_imp not in text:
    raise SystemExit('import block not found')
text = text.replace(old_imp, new_imp, 1)

marker = '''
#[cfg(test)]
#[path = "tests/request_token_stats_tests.rs"]
mod tests;
'''
if marker not in text:
    raise SystemExit('test marker not found')

# Insert methods before end of impl: find last "}\n\n#[cfg(test)]"
needle = '}\n\n#[cfg(test)]\n#[path = "tests/request_token_stats_tests.rs"]\nmod tests;\n'
if needle not in text:
    # try CRLF
    needle = '}\r\n\r\n#[cfg(test)]\r\n#[path = "tests/request_token_stats_tests.rs"]\r\nmod tests;\r\n'
    if needle not in text:
        raise SystemExit('impl end needle not found')
    insert = (
        methods
        + '\n\n'
        + backfill_and_helpers
        + '\n\n#[cfg(test)]\r\n#[path = "tests/request_token_stats_tests.rs"]\r\nmod tests;\r\n'
    )
    # methods currently start with "    pub fn..." which belongs inside impl.
    # The closing } of impl must come AFTER methods and BEFORE free functions.
    # So structure:
    #   ...existing ensure...
    #   methods
    #   backfill method (fn inside impl)
    # }
    # helpers (free fns)
    # #[cfg(test)]
    raise SystemExit('unexpected CRLF path needs manual')

# methods are inside impl; free helpers are outside.
# backfill is impl method. helpers are free.
# Split backfill_and_helpers: backfill ends before "fn cache_hit_rate"
parts = backfill_and_helpers.split('\nfn cache_hit_rate')
if len(parts) != 2:
    raise SystemExit(f'unexpected split {len(parts)}')
backfill_method = parts[0].rstrip()
helpers = 'fn cache_hit_rate' + parts[1]

replacement = (
    methods.rstrip()
    + '\n\n'
    + backfill_method
    + '\n}\n\n'
    + helpers
    + '\n\n#[cfg(test)]\n#[path = "tests/request_token_stats_tests.rs"]\nmod tests;\n'
)
text = text.replace(needle, replacement, 1)
main_path.write_text(text, encoding='utf-8')
print('injected custom daily usage + backfill + helpers into request_token_stats.rs')
print('file lines', len(text.splitlines()))

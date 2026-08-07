from pathlib import Path
path = Path(r"crates/service/src/gateway/upstream/proxy_pipeline/candidate_executor_tests.rs")
text = path.read_text(encoding="utf-8")
old = """        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(\"account_rotation\"),
        Some(setup.route_strategy_for_log),
        Some(setup.route_source_for_log),
        1,
        setup.candidate_count,
        setup.account_max_inflight,
"""
new = """        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(\"account_rotation\"),
        None,
        None,
        Some(setup.route_strategy_for_log),
        Some(setup.route_source_for_log),
        1,
        setup.candidate_count,
        setup.account_max_inflight,
"""
if old not in text:
    raise SystemExit('block not found')
path.write_text(text.replace(old, new, 1), encoding='utf-8')
print('fixed test ctor args')

from pathlib import Path
p = Path(r"crates/service/src/auth/tests/auth_account_tests.rs")
text = p.read_text(encoding="utf-8")
old = '''    let db_path = std::env::temp_dir().join(format!(
        "codexmanager-auth-target-{}-{}.sqlite",
        std::process::id(),
        now_ts()
    ));'''
new = '''    let db_path = std::env::temp_dir().join(format!(
        "codexmanager-auth-target-{}-{}-{}.sqlite",
        std::process::id(),
        now_ts(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));'''
if old not in text:
    raise SystemExit('auth path pattern not found')
p.write_text(text.replace(old, new, 1), encoding='utf-8')
print('auth path unique ok')

# verify silent gap
p2 = Path(r"crates/service/src/gateway/observability/tests/http_bridge_tests.rs")
t2 = p2.read_text(encoding='utf-8')
assert 'resp_eventsource_keepalive' in t2
idx = t2.find('resp_eventsource_keepalive')
print(repr(t2[idx:idx+250]))
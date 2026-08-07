from pathlib import Path
p = Path(r"crates/service/src/account/proxy_testing/latency.rs")
text = p.read_text(encoding="utf-8")
old = """        assert_eq!(result.status, \"ok\");
        assert_eq!(result.status_code, Some(204));
        assert!(!result.redirected);
        assert!(result.url_latency_ms.is_some());
        assert_eq!(result.error_code, None);"""
new = """        assert_eq!(
            result.status.as_str(),
            \"ok\",
            \"status={} code={:?} err_code={:?} err={:?} redirected={} latency={:?}\",
            result.status,
            result.status_code,
            result.error_code,
            result.error,
            result.redirected,
            result.url_latency_ms
        );
        assert_eq!(result.status_code, Some(204));
        assert!(!result.redirected);
        assert!(result.url_latency_ms.is_some());
        assert_eq!(result.error_code, None);"""
if old not in text:
    raise SystemExit("pattern not found")
p.write_text(text.replace(old, new, 1), encoding="utf-8")
print("ok")
from pathlib import Path

# 1) fix http_bridge/mod.rs
p = Path('crates/service/src/gateway/observability/http_bridge/mod.rs')
t = p.read_text(encoding='utf-8')
old = '''pub(super) fn reload_from_env() {
    reload_output_text_from_env();
    stream_readers::reload_from_env();
    reasoning_guard::clear_runtime_state();
}

/// 函数 `current_sse_keepalive_interval_ms`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn current_sse_keepalive_interval_ms() -> u64 {
    stream_readers::current_sse_keepalive_interval_ms()
}

/// 函数 `set_sse_keepalive_interval_ms`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn set_sse_keepalive_interval_ms(interval_ms: u64) -> Result<u64, String> {
    stream_readers::set_sse_keepalive_interval_ms(interval_ms)
}
'''
new = '''pub(super) fn reload_from_env() {
    reload_output_text_from_env();
    reasoning_guard::clear_runtime_state();
}
'''
if old not in t:
    raise SystemExit('http_bridge mod pattern missing')
p.write_text(t.replace(old, new), encoding='utf-8')
print('fixed http_bridge/mod.rs')

# 2) payload visibility
p = Path('crates/service/src/gateway/upstream/support/payload_rewrite.rs')
t = p.read_text(encoding='utf-8')
t2 = t.replace('pub(in super::super) fn body_has_encrypted_content_hint', 'pub(in crate::gateway) fn body_has_encrypted_content_hint', 1)
t2 = t2.replace('pub(in super::super) fn strip_encrypted_content_from_body', 'pub(in crate::gateway) fn strip_encrypted_content_from_body', 1)
if t2 == t:
    raise SystemExit('visibility replace failed')
p.write_text(t2, encoding='utf-8')
print('fixed payload_rewrite visibility')

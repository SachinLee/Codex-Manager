from pathlib import Path

# --- Fix latency fake proxy ---
lat = Path(r"crates/service/src/account/proxy_testing/latency.rs")
text = lat.read_text(encoding="utf-8")

# Restore clean assertions with diagnostics kept concise
old_assert = '''        assert_eq!(
            result.status.as_str(),
            "ok",
            "status={} code={:?} err_code={:?} err={:?} redirected={} latency={:?}",
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
        assert_eq!(result.error_code, None);'''
new_assert = '''        assert_eq!(
            result.status.as_str(),
            "ok",
            "status={} code={:?} err_code={:?} err={:?} redirected={} latency={:?}",
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
        assert_eq!(result.error_code, None);'''
# keep assert as-is if already diagnostic

old_proxy = '''    fn start_fake_proxy_response(
        response: &'static str,
    ) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        use std::time::{Duration, Instant};
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake proxy");
        let addr = listener.local_addr().expect("fake proxy addr");
        let proxy_url = format!("http://{addr}");
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let _ = listener.set_nonblocking(true);
            // Warmup + 10 samples (+ occasional retries) under parallel cargo test load.
            for _ in 0..24 {
                let mut stream_opt = None;
                let start_wait = Instant::now();
                while start_wait.elapsed() < Duration::from_millis(2_000) {
                    if let Ok((stream, _)) = listener.accept() {
                        stream_opt = Some(stream);
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }

                let Some(mut stream) = stream_opt else {
                    break;
                };

                let mut buffer = vec![0_u8; 8192];
                if let Ok(size) = stream.read(&mut buffer) {
                    let request = String::from_utf8_lossy(&buffer[..size]).to_string();
                    if tx.send(request).is_err() {
                        break;
                    }
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        });
        (proxy_url, rx, handle)
    }
}'''

new_proxy = '''    fn start_fake_proxy_response(
        response: &'static str,
    ) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        use std::time::{Duration, Instant};
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake proxy");
        let addr = listener.local_addr().expect("fake proxy addr");
        let proxy_url = format!("http://{addr}");
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            // Keep accepting for the whole test window. Do not exit on a single
            // idle accept timeout — client runtime warm-up can exceed 2s, and
            // the previous loop would drop the listener and cause connection errors.
            let _ = listener.set_nonblocking(true);
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut served = 0_u32;
            let mut idle_since = Instant::now();
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        idle_since = Instant::now();
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

                        // Read full request headers before responding.
                        let mut request = Vec::new();
                        let mut buf = [0_u8; 1024];
                        loop {
                            match stream.read(&mut buf) {
                                Ok(0) => break,
                                Ok(size) => {
                                    request.extend_from_slice(&buf[..size]);
                                    if request.windows(4).any(|w| w == b"\\r\\n\\r\\n") {
                                        break;
                                    }
                                }
                                Err(err)
                                    if err.kind() == std::io::ErrorKind::WouldBlock
                                        || err.kind() == std::io::ErrorKind::TimedOut =>
                                {
                                    break;
                                }
                                Err(_) => break,
                            }
                        }

                        if request.is_empty() {
                            continue;
                        }
                        let request_text = String::from_utf8_lossy(&request).to_string();
                        if tx.send(request_text).is_err() {
                            break;
                        }
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                        served += 1;
                        // Warmup + 10 samples is enough; keep a short tail for retries.
                        if served >= 16 {
                            // Stay up briefly for any in-flight retry, then exit.
                            let tail_deadline = Instant::now() + Duration::from_millis(500);
                            while Instant::now() < tail_deadline {
                                match listener.accept() {
                                    Ok((mut stream, _)) => {
                                        let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                                        let mut drain = [0_u8; 1024];
                                        let _ = stream.read(&mut drain);
                                        let _ = stream.write_all(response.as_bytes());
                                    }
                                    Err(_) => thread::sleep(Duration::from_millis(10)),
                                }
                            }
                            break;
                        }
                    }
                    Err(err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            || err.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        // Exit early once we have served traffic and gone idle.
                        if served > 0 && idle_since.elapsed() > Duration::from_secs(3) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        (proxy_url, rx, handle)
    }
}'''

if old_proxy not in text:
    raise SystemExit('latency proxy pattern not found')
text = text.replace(old_proxy, new_proxy, 1)

# Also bump proxy test runtime workers for parallel latency tests
old_rt = '''    PROXY_TEST_RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .thread_name("proxy-test-http")
            .build()
            .unwrap_or_else(|err| panic!("build proxy test runtime failed: {err}"))
    })'''
new_rt = '''    PROXY_TEST_RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("proxy-test-http")
            .build()
            .unwrap_or_else(|err| panic!("build proxy test runtime failed: {err}"))
    })'''
if old_rt in text:
    text = text.replace(old_rt, new_rt, 1)
    print('runtime workers bumped')
else:
    print('runtime pattern not found (skip)')

lat.write_text(text, encoding='utf-8')
print('latency fixed')

# --- Fix keepalive silent gap ---
hb = Path(r"crates/service/src/gateway/observability/tests/http_bridge_tests.rs")
hb_text = hb.read_text(encoding='utf-8')
# Only the failing test uses 50ms delay before DONE in this exact context
old_ka = '''fn passthrough_sse_reader_emits_keepalive_for_responses_stream() {
    let _guard = crate::test_env_guard();
    let _reload_guard = RuntimeConfigReloadGuard;
    let _enabled_guard = EnvGuard::set("CODEXMANAGER_SSE_KEEPALIVE_ENABLED", "1");
    let _keepalive_guard = EnvGuard::set("CODEXMANAGER_SSE_KEEPALIVE_INTERVAL_MS", "15");
    crate::gateway::reload_runtime_config_from_env();

    let (upstream, server) = open_streaming_mock_http_response(
        "text/event-stream",
        &[
            (
                "data: {\\"type\\":\\"response.created\\",\\"response\\":{\\"id\\":\\"resp_keepalive_1\\"}}\\n\\n",
                0,
            ),
            ("data: [DONE]\\n\\n", 50),
        ],
    );'''
new_ka = '''fn passthrough_sse_reader_emits_keepalive_for_responses_stream() {
    let _guard = crate::test_env_guard();
    let _reload_guard = RuntimeConfigReloadGuard;
    let _enabled_guard = EnvGuard::set("CODEXMANAGER_SSE_KEEPALIVE_ENABLED", "1");
    let _keepalive_guard = EnvGuard::set("CODEXMANAGER_SSE_KEEPALIVE_INTERVAL_MS", "15");
    crate::gateway::reload_runtime_config_from_env();

    let (upstream, server) = open_streaming_mock_http_response(
        "text/event-stream",
        &[
            (
                "data: {\\"type\\":\\"response.created\\",\\"response\\":{\\"id\\":\\"resp_keepalive_1\\"}}\\n\\n",
                0,
            ),
            // Keep gap well above interval so pump setup/CPU jitter cannot collapse the silent window.
            ("data: [DONE]\\n\\n", 200),
        ],
    );'''
if old_ka not in hb_text:
    raise SystemExit('keepalive pattern not found')
hb.write_text(hb_text.replace(old_ka, new_ka, 1), encoding='utf-8')
print('keepalive fixed')
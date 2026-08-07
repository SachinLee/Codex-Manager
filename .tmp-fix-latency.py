from pathlib import Path
path = Path(r"crates/service/src/account/proxy_testing/latency.rs")
text = path.read_text(encoding="utf-8")
old = '''            for _ in 0..10 {
                let mut stream_opt = None;
                let start_wait = Instant::now();
                while start_wait.elapsed() < Duration::from_millis(500) {
'''
new = '''            // Warmup + 10 samples (+ occasional retries) under parallel cargo test load.
            for _ in 0..24 {
                let mut stream_opt = None;
                let start_wait = Instant::now();
                while start_wait.elapsed() < Duration::from_millis(2_000) {
'''
if old not in text:
    raise SystemExit('latency fake proxy block not found')
path.write_text(text.replace(old, new, 1), encoding='utf-8')
print('latency fake proxy capacity fixed')

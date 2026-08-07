from pathlib import Path
p = Path(r"crates/service/src/account/proxy_testing/latency.rs")
text = p.read_text(encoding="utf-8")
old = '''                    Ok((mut stream, _)) => {
                        idle_since = Instant::now();
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));'''
new = '''                    Ok((mut stream, _)) => {
                        idle_since = Instant::now();
                        // Accepted sockets may inherit nonblocking from the listener.
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));'''
if old not in text:
    raise SystemExit('accept block not found')
p.write_text(text.replace(old, new, 1), encoding='utf-8')
print('blocking set added')
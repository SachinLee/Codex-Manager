# Desktop empty-profile baseline

- Scope: approved Windows desktop startup and idle measurement only.
- Binary: `target/release/CodexManager-0.5.3.4.exe`.
- Isolation: process-local `CODEXMANAGER_DB_PATH`, `CODEXMANAGER_RPC_TOKEN_FILE`, `CODEXMANAGER_SERVICE_ADDR=localhost:48762`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, and `TMP` all point below this run's `sandbox/` directory.
- Guard: an unrelated existing CodexManager instance owns port 48760; it is not queried, stopped, or otherwise changed.
- Workload: fresh empty database. No account, key, or paid upstream is configured. Results can establish startup/empty-idle cost only; they cannot represent active gateway-request cost.
- Required signals: Tauri host PID, WebView2 process relationship, process CPU time, private bytes, private working set, and thread count over a 90-second ready interval.

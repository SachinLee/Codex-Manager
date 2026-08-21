# Dynamic baseline blocker

## Result

No dynamic resource sample was accepted.

## Evidence

- The approved release binary was launched with process-local database, RPC token, service address, AppData, LocalAppData, Temp, and Tmp paths under this measurement run's sandbox.
- The launch exited with code 0 before the isolated service port `48762` became ready and emitted no process log.
- Source confirms `tauri_plugin_single_instance` intercepts a secondary instance before `.setup()` runs (`apps/src-tauri/src/lib.rs:97-114`).
- A pre-existing primary `CodexManager-0.5.3.4` instance owns the default service port. It was not stopped, inspected beyond process identity/port ownership, or sampled.
- No portable or QA-identifier desktop executable exists in the available release outputs.

## Conclusion boundary

The sandbox database and alternate port are insufficient to bypass the application-wide single-instance guard. The attempted process never reached storage setup, so no PID tree, WebView2 relationship, startup interval, or idle resource value is valid.

## Safe resume conditions

Use either a dedicated QA executable with a distinct application identifier, or voluntarily close the existing primary CodexManager instance before launching the same sandboxed binary. Keep the current isolated database/token/service-address environment and do not use real accounts or upstreams.

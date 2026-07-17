use codexmanager_core::storage::GatewayUpstreamAttemptEvent;

#[cfg(not(test))]
use std::sync::{
    mpsc::{sync_channel, SyncSender, TrySendError},
    OnceLock,
};

#[cfg(not(test))]
const CAPABILITY_ATTEMPT_EVENT_QUEUE_CAPACITY: usize = 1024;

#[cfg(not(test))]
static CAPABILITY_ATTEMPT_EVENT_SENDER: OnceLock<SyncSender<GatewayUpstreamAttemptEvent>> =
    OnceLock::new();

pub(crate) fn record_gateway_capability_attempt_event(event: GatewayUpstreamAttemptEvent) {
    #[cfg(test)]
    insert_gateway_capability_attempt_event(&event);

    #[cfg(not(test))]
    match capability_attempt_event_sender().try_send(event) {
        Ok(()) => {}
        Err(TrySendError::Full(event)) | Err(TrySendError::Disconnected(event)) => {
            insert_gateway_capability_attempt_event(&event);
        }
    }
}

#[cfg(not(test))]
fn capability_attempt_event_sender() -> &'static SyncSender<GatewayUpstreamAttemptEvent> {
    CAPABILITY_ATTEMPT_EVENT_SENDER.get_or_init(|| {
        let (tx, rx) = sync_channel(CAPABILITY_ATTEMPT_EVENT_QUEUE_CAPACITY);
        let _ = std::thread::Builder::new()
            .name("cm-capability-attempt-events".to_string())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    insert_gateway_capability_attempt_event(&event);
                }
            });
        tx
    })
}

fn insert_gateway_capability_attempt_event(event: &GatewayUpstreamAttemptEvent) {
    let Some(storage) = crate::storage_helpers::open_storage() else {
        log::warn!(
            "event=gateway_capability_attempt_insert_skipped trace_id={} source_id={} reason=storage_unavailable",
            event.trace_id,
            event.source_id
        );
        return;
    };
    if let Err(err) = storage.insert_gateway_upstream_attempt_event(event) {
        log::warn!(
            "event=gateway_capability_attempt_insert_failed trace_id={} source_id={} err={}",
            event.trace_id,
            event.source_id,
            err
        );
    }
}

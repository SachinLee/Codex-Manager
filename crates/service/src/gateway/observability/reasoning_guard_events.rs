use codexmanager_core::storage::GatewayReasoningGuardEvent;

#[cfg(not(test))]
use std::sync::{
    mpsc::{sync_channel, SyncSender, TrySendError},
    OnceLock,
};

#[cfg(not(test))]
const REASONING_GUARD_EVENT_QUEUE_CAPACITY: usize = 1024;

#[cfg(not(test))]
static REASONING_GUARD_EVENT_SENDER: OnceLock<SyncSender<GatewayReasoningGuardEvent>> =
    OnceLock::new();

pub(crate) fn record_gateway_reasoning_guard_event(event: GatewayReasoningGuardEvent) {
    #[cfg(test)]
    {
        insert_gateway_reasoning_guard_event(&event);
    }

    #[cfg(not(test))]
    {
        match reasoning_guard_event_sender().try_send(event) {
            Ok(()) => {}
            Err(TrySendError::Full(event)) | Err(TrySendError::Disconnected(event)) => {
                insert_gateway_reasoning_guard_event(&event);
            }
        }
    }
}

#[cfg(not(test))]
fn reasoning_guard_event_sender() -> &'static SyncSender<GatewayReasoningGuardEvent> {
    REASONING_GUARD_EVENT_SENDER.get_or_init(|| {
        let (tx, rx) = sync_channel(REASONING_GUARD_EVENT_QUEUE_CAPACITY);
        let _ = std::thread::Builder::new()
            .name("cm-reasoning-guard-events".to_string())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    insert_gateway_reasoning_guard_event(&event);
                }
            });
        tx
    })
}

fn insert_gateway_reasoning_guard_event(event: &GatewayReasoningGuardEvent) {
    let Some(storage) = crate::storage_helpers::open_storage() else {
        log::warn!(
            "event=gateway_reasoning_guard_event_insert_skipped trace_id={} action={} reason=storage_unavailable",
            event.trace_id.as_deref().unwrap_or_default(),
            event.action
        );
        return;
    };
    if let Err(err) = storage.insert_gateway_reasoning_guard_event(event) {
        log::warn!(
            "event=gateway_reasoning_guard_event_insert_failed trace_id={} action={} err={}",
            event.trace_id.as_deref().unwrap_or_default(),
            event.action,
            err
        );
    }
}

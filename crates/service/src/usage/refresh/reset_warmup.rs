use codexmanager_core::storage::{now_ts, AccountResetWarmupTarget, Storage};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::account_warmup::AccountWarmupItemResult;
use crate::storage_helpers::open_storage;

const RESET_WARMUP_POLL_INTERVAL: Duration = Duration::from_secs(5);
const RESET_WARMUP_WORKERS: usize = 4;
const RESET_WARMUP_QUEUE_CAPACITY: usize = 128;
type PendingTasks = Arc<Mutex<HashSet<(String, i64)>>>;

/// Quota reset deadlines have their own clock so slow usage polling, polling
/// failures and the optional user cron never delay the start of a new window.
pub(super) fn reset_warmup_loop() {
    let executor = match ResetWarmupExecutor::new() {
        Ok(executor) => executor,
        Err(err) => {
            log::error!("account reset warmup workers unavailable: {err}");
            return;
        }
    };
    while !crate::shutdown_requested() {
        if let Err(err) = enqueue_due_reset_warmups(&executor) {
            log::warn!("account reset warmup scheduling failed: {err}");
        }
        let deadline = std::time::Instant::now() + RESET_WARMUP_POLL_INTERVAL;
        while !crate::shutdown_requested() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(250));
        }
    }
}

fn enqueue_due_reset_warmups(executor: &ResetWarmupExecutor) -> Result<(), String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let targets = storage
        .list_account_reset_warmup_targets(now_ts(), RESET_WARMUP_QUEUE_CAPACITY)
        .map_err(|err| err.to_string())?;
    for target in targets {
        if crate::shutdown_requested() {
            break;
        }
        executor.enqueue(target);
    }
    Ok(())
}

struct ResetWarmupExecutor {
    sender: Sender<AccountResetWarmupTarget>,
    pending: PendingTasks,
}

impl ResetWarmupExecutor {
    fn new() -> Result<Self, String> {
        let (sender, receiver) = bounded(RESET_WARMUP_QUEUE_CAPACITY);
        let pending = Arc::new(Mutex::new(HashSet::new()));
        for index in 0..RESET_WARMUP_WORKERS {
            let receiver = receiver.clone();
            let pending = Arc::clone(&pending);
            thread::Builder::new()
                .name(format!("account-reset-warmup-{index}"))
                .spawn(move || reset_warmup_worker(receiver, pending))
                .map_err(|err| err.to_string())?;
        }
        Ok(Self { sender, pending })
    }

    fn enqueue(&self, target: AccountResetWarmupTarget) -> bool {
        let key = (target.account_id.clone(), target.reset_at);
        if !crate::lock_utils::lock_recover(&self.pending, "pending_reset_warmups")
            .insert(key.clone())
        {
            return false;
        }
        if self.sender.try_send(target).is_err() {
            crate::lock_utils::lock_recover(&self.pending, "pending_reset_warmups").remove(&key);
            return false;
        }
        true
    }
}

fn reset_warmup_worker(receiver: Receiver<AccountResetWarmupTarget>, pending: PendingTasks) {
    while !crate::shutdown_requested() {
        let target = match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(target) => target,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        };
        let key = (target.account_id.clone(), target.reset_at);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
            run_reset_warmup_task(&storage, &target, now_ts(), |storage, account_id| {
                crate::account_warmup::warmup_account_after_reset(storage, account_id)
            })
        }));
        match outcome {
            Ok(Ok(Some(item))) => log::info!(
                "account reset warmup finished: account_id={} reset_at={} ok={}",
                item.account_id,
                target.reset_at,
                item.ok
            ),
            Ok(Ok(None)) => {}
            Ok(Err(err)) => log::warn!(
                "account reset warmup failed: account_id={} reset_at={} err={}",
                target.account_id,
                target.reset_at,
                err
            ),
            Err(_) => log::error!(
                "account reset warmup worker panicked: account_id={} reset_at={}",
                target.account_id,
                target.reset_at
            ),
        }
        crate::lock_utils::lock_recover(&pending, "pending_reset_warmups").remove(&key);
    }
}

fn run_reset_warmup_task<F>(
    storage: &Storage,
    target: &AccountResetWarmupTarget,
    now: i64,
    send: F,
) -> Result<Option<AccountWarmupItemResult>, String>
where
    F: FnOnce(&Storage, &str) -> Result<AccountWarmupItemResult, String>,
{
    if target.due_at > now || crate::shutdown_requested() {
        return Ok(None);
    }
    // Claim only at execution, never at enqueue time. The transaction rechecks
    // the per-account switch, status, current quota and cycle, and persists the
    // attempt before HTTP so restarts and concurrent services cannot resend it.
    if !storage
        .claim_account_reset_warmup(&target.account_id, target.reset_at, now)
        .map_err(|err| err.to_string())?
    {
        return Ok(None);
    }
    send(storage, &target.account_id).map(Some)
}

#[cfg(test)]
#[path = "reset_warmup_tests.rs"]
mod tests;

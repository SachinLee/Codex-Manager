use rusqlite::{params, OptionalExtension, Result};

use super::{
    aggregate_apis_sql::update_aggregate_api_balance_result_sql, now_ts,
    AggregateApiZeroBalanceState, AggregateApiZeroBalanceStateKind,
    AggregateApiZeroBalanceTransition, Storage,
};

const ZERO_BALANCE_STATE_BY_ID_SQL: &str = "SELECT aggregate_api_id, state, observed_at, released_at, updated_at FROM aggregate_api_zero_balance_route_states WHERE aggregate_api_id = ?1";

fn map_zero_balance_state(row: &rusqlite::Row<'_>) -> Result<AggregateApiZeroBalanceState> {
    let state = match row.get::<_, String>(1)?.as_str() {
        "zero_balance_blocked" => AggregateApiZeroBalanceStateKind::ZeroBalanceBlocked,
        "manually_released" => AggregateApiZeroBalanceStateKind::ManuallyReleased,
        _ => return Err(rusqlite::Error::QueryReturnedNoRows),
    };

    Ok(AggregateApiZeroBalanceState {
        aggregate_api_id: row.get(0)?,
        state,
        observed_at: row.get(2)?,
        released_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

impl Storage {
    pub fn aggregate_api_zero_balance_state(
        &self,
        api_id: &str,
    ) -> Result<Option<AggregateApiZeroBalanceState>> {
        self.conn
            .query_row(
                ZERO_BALANCE_STATE_BY_ID_SQL,
                [api_id],
                map_zero_balance_state,
            )
            .optional()
    }

    pub fn list_aggregate_api_zero_balance_states(
        &self,
    ) -> Result<Vec<AggregateApiZeroBalanceState>> {
        let mut statement = self.conn.prepare(
            "SELECT aggregate_api_id, state, observed_at, released_at, updated_at
             FROM aggregate_api_zero_balance_route_states
             ORDER BY aggregate_api_id",
        )?;
        statement.query_map([], map_zero_balance_state)?.collect()
    }

    pub fn list_zero_balance_blocked_aggregate_api_ids(&self) -> Result<Vec<String>> {
        let mut statement = self.conn.prepare(
            "SELECT aggregate_api_id
             FROM aggregate_api_zero_balance_route_states
             WHERE state = ?1
             ORDER BY aggregate_api_id",
        )?;
        statement
            .query_map(["zero_balance_blocked"], |row| row.get(0))?
            .collect()
    }

    pub fn release_aggregate_api_zero_balance_state(
        &self,
        api_id: &str,
        released_at: i64,
    ) -> Result<Option<AggregateApiZeroBalanceState>> {
        let tx = self.conn.unchecked_transaction()?;
        let state = tx
            .query_row(
                ZERO_BALANCE_STATE_BY_ID_SQL,
                [api_id],
                map_zero_balance_state,
            )
            .optional()?;

        let result = match state {
            Some(state) if state.state == AggregateApiZeroBalanceStateKind::ZeroBalanceBlocked => {
                let changed = tx.execute(
                    "UPDATE aggregate_api_zero_balance_route_states
                     SET state = 'manually_released', released_at = ?1, updated_at = ?1
                     WHERE aggregate_api_id = ?2 AND state = 'zero_balance_blocked'",
                    params![released_at, api_id],
                )?;

                (changed == 1).then_some(AggregateApiZeroBalanceState {
                    aggregate_api_id: state.aggregate_api_id,
                    state: AggregateApiZeroBalanceStateKind::ManuallyReleased,
                    observed_at: state.observed_at,
                    released_at: Some(released_at),
                    updated_at: released_at,
                })
            }
            Some(state) => Some(state),
            None => None,
        };

        tx.commit()?;
        Ok(result)
    }

    pub fn update_aggregate_api_balance_result_with_zero_balance_state(
        &self,
        api_id: &str,
        ok: bool,
        balance_json: Option<&str>,
        error: Option<&str>,
        transition: AggregateApiZeroBalanceTransition,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let now = now_ts();
        let status = if ok { Some("success") } else { Some("failed") };
        let updated = tx.execute(
            update_aggregate_api_balance_result_sql(),
            (now, status, error, balance_json, api_id),
        )?;
        if updated != 1 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        let balance_query_enabled = tx.query_row(
            "SELECT balance_query_enabled FROM aggregate_apis WHERE id = ?1",
            [api_id],
            |row| row.get::<_, bool>(0),
        )?;
        if balance_query_enabled {
            match transition {
                AggregateApiZeroBalanceTransition::Block { observed_at } => {
                    tx.execute(
                        "INSERT INTO aggregate_api_zero_balance_route_states
                         (aggregate_api_id, state, observed_at, released_at, updated_at)
                         VALUES (?1, 'zero_balance_blocked', ?2, NULL, ?3)
                         ON CONFLICT(aggregate_api_id) DO UPDATE SET
                           state = excluded.state,
                           observed_at = excluded.observed_at,
                           released_at = NULL,
                           updated_at = excluded.updated_at",
                        params![api_id, observed_at, now],
                    )?;
                }
                AggregateApiZeroBalanceTransition::Clear => {
                    tx.execute(
                        "DELETE FROM aggregate_api_zero_balance_route_states
                         WHERE aggregate_api_id = ?1",
                        [api_id],
                    )?;
                }
                AggregateApiZeroBalanceTransition::Preserve => {}
            }
        }

        tx.commit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{now_ts, AggregateApi};

    fn aggregate_api(id: &str) -> AggregateApi {
        AggregateApi {
            id: id.to_string(),
            provider_type: "codex".to_string(),
            supplier_name: Some("test".to_string()),
            sort: 0,
            url: "https://example.invalid/v1".to_string(),
            auth_type: "apikey".to_string(),
            auth_params_json: None,
            action: None,
            model_override: None,
            cost_multiplier: 1.0,
            daily_spend_limit_usd: None,
            status: "active".to_string(),
            created_at: now_ts(),
            updated_at: now_ts(),
            last_test_at: None,
            last_test_status: None,
            last_test_error: None,
            balance_query_enabled: true,
            balance_query_template: Some("generic".to_string()),
            balance_query_base_url: None,
            balance_query_user_id: None,
            balance_query_config_json: None,
            last_balance_at: None,
            last_balance_status: None,
            last_balance_error: None,
            last_balance_json: None,
        }
    }

    #[test]
    fn zero_balance_state_and_manual_release_survive_reopen() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "codexmanager-aggregate-api-zero-balance-{}-{nonce}.db",
            std::process::id()
        ));
        let storage = Storage::open(&path).expect("open storage");
        storage.init().expect("initialize storage");
        storage
            .insert_aggregate_api(&aggregate_api("agg-zero"))
            .expect("insert aggregate API");

        storage
            .update_aggregate_api_balance_result_with_zero_balance_state(
                "agg-zero",
                true,
                Some(r#"{\"isValid\":true,\"remaining\":0}"#),
                None,
                AggregateApiZeroBalanceTransition::Block { observed_at: 100 },
            )
            .expect("persist zero-balance state");
        drop(storage);

        let reopened = Storage::open(&path).expect("reopen storage");
        reopened.init().expect("initialize reopened storage");
        let state = reopened
            .aggregate_api_zero_balance_state("agg-zero")
            .expect("read state")
            .expect("zero-balance state");
        assert_eq!(
            state.state,
            AggregateApiZeroBalanceStateKind::ZeroBalanceBlocked
        );
        assert_eq!(state.observed_at, 100);
        assert!(reopened
            .list_zero_balance_blocked_aggregate_api_ids()
            .expect("list blocked API IDs")
            .iter()
            .any(|id| id == "agg-zero"));

        let released = reopened
            .release_aggregate_api_zero_balance_state("agg-zero", 101)
            .expect("release zero-balance state")
            .expect("released state");
        assert_eq!(
            released.state,
            AggregateApiZeroBalanceStateKind::ManuallyReleased
        );
        drop(reopened);

        let reopened_after_release = Storage::open(&path).expect("reopen released storage");
        reopened_after_release
            .init()
            .expect("initialize released storage");
        let released_state = reopened_after_release
            .aggregate_api_zero_balance_state("agg-zero")
            .expect("read released state")
            .expect("manually released state");
        assert_eq!(
            released_state.state,
            AggregateApiZeroBalanceStateKind::ManuallyReleased
        );
        assert_eq!(released_state.released_at, Some(101));
        assert!(reopened_after_release
            .list_zero_balance_blocked_aggregate_api_ids()
            .expect("list blocked API IDs")
            .is_empty());
        drop(reopened_after_release);
        let mut cleanup_error = None;
        for attempt in 0..20 {
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    cleanup_error = None;
                    break;
                }
                Err(error) => {
                    cleanup_error = Some(error);
                    if attempt < 19 {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
            }
        }
        if let Some(error) = cleanup_error {
            panic!("remove temporary storage: {error}");
        }
    }

    #[test]
    fn successful_positive_balance_clears_but_unknown_results_preserve_zero_balance_state() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage.init().expect("initialize storage");
        storage
            .insert_aggregate_api(&aggregate_api("agg-transition"))
            .expect("insert aggregate API");

        storage
            .update_aggregate_api_balance_result_with_zero_balance_state(
                "agg-transition",
                true,
                Some(r#"{\"isValid\":true,\"remaining\":0}"#),
                None,
                AggregateApiZeroBalanceTransition::Block { observed_at: 100 },
            )
            .expect("block zero balance");
        storage
            .update_aggregate_api_balance_result_with_zero_balance_state(
                "agg-transition",
                false,
                None,
                Some("balance query failed"),
                AggregateApiZeroBalanceTransition::Preserve,
            )
            .expect("preserve zero-balance state");
        assert!(storage
            .aggregate_api_zero_balance_state("agg-transition")
            .expect("read preserved state")
            .is_some());

        storage
            .update_aggregate_api_balance_result_with_zero_balance_state(
                "agg-transition",
                true,
                Some(r#"{\"isValid\":true,\"remaining\":1}"#),
                None,
                AggregateApiZeroBalanceTransition::Clear,
            )
            .expect("clear zero-balance state");
        assert!(storage
            .aggregate_api_zero_balance_state("agg-transition")
            .expect("read cleared state")
            .is_none());
    }

    #[test]
    fn disabling_balance_queries_clears_existing_zero_balance_state() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage.init().expect("initialize storage");
        storage
            .insert_aggregate_api(&aggregate_api("agg-disabled"))
            .expect("insert aggregate API");
        storage
            .update_aggregate_api_balance_result_with_zero_balance_state(
                "agg-disabled",
                true,
                Some(r#"{\"isValid\":true,\"remaining\":0}"#),
                None,
                AggregateApiZeroBalanceTransition::Block { observed_at: 100 },
            )
            .expect("block zero balance");

        storage
            .update_aggregate_api_balance_query(
                "agg-disabled",
                false,
                Some("generic"),
                None,
                None,
                None,
            )
            .expect("disable balance query");

        assert!(storage
            .aggregate_api_zero_balance_state("agg-disabled")
            .expect("read cleared state")
            .is_none());
    }
}

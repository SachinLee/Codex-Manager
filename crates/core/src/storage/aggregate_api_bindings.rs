use rusqlite::params;

use super::{AggregateApiBinding, Storage};

pub(super) fn aggregate_api_binding_lookup_sql() -> &'static str {
    "SELECT
        platform_key_hash,
        protocol_type,
        model,
        cache_affinity_route_id_hash,
        bound_aggregate_api_id,
        bound_at,
        reason
     FROM aggregate_api_bindings
     WHERE platform_key_hash = ?1
       AND protocol_type = ?2
       AND model = ?3
       AND cache_affinity_route_id_hash = ?4
     LIMIT 1"
}

fn upsert_aggregate_api_binding_sql() -> &'static str {
    "INSERT INTO aggregate_api_bindings (
        platform_key_hash,
        protocol_type,
        model,
        cache_affinity_route_id_hash,
        bound_aggregate_api_id,
        bound_at,
        reason
     )
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
     ON CONFLICT (platform_key_hash, protocol_type, model, cache_affinity_route_id_hash)
     DO UPDATE SET
        bound_aggregate_api_id = excluded.bound_aggregate_api_id,
        bound_at = excluded.bound_at,
        reason = excluded.reason"
}

pub(super) fn delete_stale_aggregate_api_bindings_sql() -> &'static str {
    "DELETE FROM aggregate_api_bindings
     WHERE bound_at < ?1"
}

impl Storage {
    /// 函数 `get_aggregate_api_binding`
    ///
    /// 作者: AI Assistant
    ///
    /// 时间: 2026-09-03
    ///
    /// # 参数
    /// - self: Storage instance
    /// - platform_key_hash: Platform key hash for partition
    /// - protocol_type: Protocol type (e.g., "openai_compat")
    /// - model: Model identifier
    /// - cache_affinity_route_id_hash: Hashed cache affinity route ID
    ///
    /// # 返回
    /// Option containing the binding if found
    pub fn get_aggregate_api_binding(
        &self,
        platform_key_hash: &str,
        protocol_type: &str,
        model: &str,
        cache_affinity_route_id_hash: &str,
    ) -> rusqlite::Result<Option<AggregateApiBinding>> {
        let mut stmt = self.conn.prepare(aggregate_api_binding_lookup_sql())?;
        let mut rows = stmt.query([
            platform_key_hash,
            protocol_type,
            model,
            cache_affinity_route_id_hash,
        ])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(AggregateApiBinding {
                platform_key_hash: row.get(0)?,
                protocol_type: row.get(1)?,
                model: row.get(2)?,
                cache_affinity_route_id_hash: row.get(3)?,
                bound_aggregate_api_id: row.get(4)?,
                bound_at: row.get(5)?,
                reason: row.get(6)?,
            }));
        }
        Ok(None)
    }

    /// 函数 `upsert_aggregate_api_binding`
    ///
    /// 作者: AI Assistant
    ///
    /// 时间: 2026-09-03
    ///
    /// # 参数
    /// - self: Storage instance
    /// - binding: Aggregate API binding to insert or update
    ///
    /// # 返回
    /// Result indicating success or failure
    pub fn upsert_aggregate_api_binding(
        &self,
        binding: &AggregateApiBinding,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            upsert_aggregate_api_binding_sql(),
            params![
                binding.platform_key_hash,
                binding.protocol_type,
                binding.model,
                binding.cache_affinity_route_id_hash,
                binding.bound_aggregate_api_id,
                binding.bound_at,
                binding.reason,
            ],
        )?;
        Ok(())
    }

    /// 函数 `delete_stale_aggregate_api_bindings`
    ///
    /// 作者: AI Assistant
    ///
    /// 时间: 2026-09-03
    ///
    /// # 参数
    /// - self: Storage instance
    /// - before_timestamp: Delete bindings older than this timestamp
    ///
    /// # 返回
    /// Number of deleted rows
    pub fn delete_stale_aggregate_api_bindings(
        &self,
        before_timestamp: i64,
    ) -> rusqlite::Result<usize> {
        self.conn
            .execute(delete_stale_aggregate_api_bindings_sql(), [before_timestamp])
    }
}

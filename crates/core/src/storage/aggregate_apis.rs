use rusqlite::{params, Result, Row};

use super::{now_ts, AggregateApi, AggregateApiSupplierModel, Storage};

const AGGREGATE_API_SELECT_SQL: &str = "SELECT
    id,
    provider_type,
    supplier_name,
    sort,
    url,
    auth_type,
    auth_params_json,
    action,
    model_override,
    cost_multiplier,
    daily_spend_limit_usd,
    status,
    created_at,
    updated_at,
    last_test_at,
    last_test_status,
    last_test_error,
    balance_query_enabled,
    balance_query_template,
    balance_query_base_url,
    balance_query_user_id,
    balance_query_config_json,
    last_balance_at,
    last_balance_status,
    last_balance_error,
    last_balance_json
 FROM aggregate_apis";

impl Storage {
    /// 函数 `insert_aggregate_api`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api: 参数 api
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn insert_aggregate_api(&self, api: &AggregateApi) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO aggregate_apis (
                id,
                provider_type,
                supplier_name,
                sort,
                url,
                auth_type,
                auth_params_json,
                action,
                model_override,
                cost_multiplier,
                daily_spend_limit_usd,
                status,
                created_at,
                updated_at,
                last_test_at,
                last_test_status,
                last_test_error,
                balance_query_enabled,
                balance_query_template,
                balance_query_base_url,
                balance_query_user_id,
                balance_query_config_json,
                last_balance_at,
                last_balance_status,
                last_balance_error,
                last_balance_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
            params![
                &api.id,
                &api.provider_type,
                &api.supplier_name,
                api.sort,
                &api.url,
                &api.auth_type,
                &api.auth_params_json,
                &api.action,
                &api.model_override,
                api.cost_multiplier,
                api.daily_spend_limit_usd,
                &api.status,
                api.created_at,
                api.updated_at,
                &api.last_test_at,
                &api.last_test_status,
                &api.last_test_error,
                api.balance_query_enabled,
                &api.balance_query_template,
                &api.balance_query_base_url,
                &api.balance_query_user_id,
                &api.balance_query_config_json,
                &api.last_balance_at,
                &api.last_balance_status,
                &api.last_balance_error,
                &api.last_balance_json,
            ],
        )?;
        Ok(())
    }

    /// 函数 `list_aggregate_apis`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn list_aggregate_apis(&self) -> Result<Vec<AggregateApi>> {
        let mut stmt = self.conn.prepare(&format!(
            "{AGGREGATE_API_SELECT_SQL} ORDER BY sort ASC, updated_at DESC"
        ))?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(map_aggregate_api_row(row)?);
        }
        Ok(out)
    }

    /// 函数 `find_aggregate_api_by_id`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn find_aggregate_api_by_id(&self, api_id: &str) -> Result<Option<AggregateApi>> {
        let mut stmt = self.conn.prepare(&format!(
            "{AGGREGATE_API_SELECT_SQL}
             WHERE id = ?1
             LIMIT 1"
        ))?;
        let mut rows = stmt.query([api_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(map_aggregate_api_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// 函数 `update_aggregate_api`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    /// - url: 参数 url
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_aggregate_api(&self, api_id: &str, url: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET url = ?1, updated_at = ?2 WHERE id = ?3",
            (url, now_ts(), api_id),
        )?;
        Ok(())
    }

    /// 函数 `update_aggregate_api_supplier_name`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    /// - supplier_name: 参数 supplier_name
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_aggregate_api_supplier_name(
        &self,
        api_id: &str,
        supplier_name: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET supplier_name = ?1, updated_at = ?2 WHERE id = ?3",
            (supplier_name, now_ts(), api_id),
        )?;
        Ok(())
    }

    /// 函数 `update_aggregate_api_sort`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    /// - sort: 参数 sort
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_aggregate_api_sort(&self, api_id: &str, sort: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET sort = ?1, updated_at = ?2 WHERE id = ?3",
            (sort, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_status(&self, api_id: &str, status: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET status = ?1, updated_at = ?2 WHERE id = ?3",
            (status, now_ts(), api_id),
        )?;
        Ok(())
    }

    /// 函数 `update_aggregate_api_type`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    /// - provider_type: 参数 provider_type
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_aggregate_api_type(&self, api_id: &str, provider_type: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET provider_type = ?1, updated_at = ?2 WHERE id = ?3",
            (provider_type, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_auth_type(&self, api_id: &str, auth_type: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET auth_type = ?1, updated_at = ?2 WHERE id = ?3",
            (auth_type, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_auth_params_json(
        &self,
        api_id: &str,
        auth_params_json: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET auth_params_json = ?1, updated_at = ?2 WHERE id = ?3",
            (auth_params_json, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_action(&self, api_id: &str, action: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET action = ?1, updated_at = ?2 WHERE id = ?3",
            (action, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_model_override(
        &self,
        api_id: &str,
        model_override: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET model_override = ?1, updated_at = ?2 WHERE id = ?3",
            (model_override, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_cost_multiplier(
        &self,
        api_id: &str,
        cost_multiplier: f64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET cost_multiplier = ?1, updated_at = ?2 WHERE id = ?3",
            (cost_multiplier, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_daily_spend_limit_usd(
        &self,
        api_id: &str,
        daily_spend_limit_usd: Option<f64>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET daily_spend_limit_usd = ?1, updated_at = ?2 WHERE id = ?3",
            (daily_spend_limit_usd, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_balance_query(
        &self,
        api_id: &str,
        enabled: bool,
        template: Option<&str>,
        base_url: Option<&str>,
        user_id: Option<&str>,
        config_json: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis
             SET balance_query_enabled = ?1,
                 balance_query_template = ?2,
                 balance_query_base_url = ?3,
                 balance_query_user_id = ?4,
                 balance_query_config_json = ?5,
                 updated_at = ?6
             WHERE id = ?7",
            (
                enabled,
                template,
                base_url,
                user_id,
                config_json,
                now_ts(),
                api_id,
            ),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_balance_result(
        &self,
        api_id: &str,
        ok: bool,
        balance_json: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let now = now_ts();
        let status = if ok { Some("success") } else { Some("failed") };
        self.conn.execute(
            "UPDATE aggregate_apis
             SET last_balance_at = ?1,
                 last_balance_status = ?2,
                 last_balance_error = ?3,
                 last_balance_json = ?4,
                 updated_at = ?1
             WHERE id = ?5",
            (now, status, error, balance_json, api_id),
        )?;
        Ok(())
    }

    /// 函数 `delete_aggregate_api`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn delete_aggregate_api(&self, api_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM aggregate_api_balance_secrets WHERE aggregate_api_id = ?1",
            [api_id],
        )?;
        self.conn.execute(
            "DELETE FROM aggregate_api_secrets WHERE aggregate_api_id = ?1",
            [api_id],
        )?;
        self.conn
            .execute("DELETE FROM aggregate_apis WHERE id = ?1", [api_id])?;
        Ok(())
    }

    /// 函数 `upsert_aggregate_api_secret`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    /// - secret_value: 参数 secret_value
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn upsert_aggregate_api_secret(&self, api_id: &str, secret_value: &str) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO aggregate_api_secrets (aggregate_api_id, secret_value, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(aggregate_api_id) DO UPDATE SET
               secret_value = excluded.secret_value,
               updated_at = excluded.updated_at",
            (api_id, secret_value, now),
        )?;
        Ok(())
    }

    /// 函数 `find_aggregate_api_secret_by_id`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn find_aggregate_api_secret_by_id(&self, api_id: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT secret_value FROM aggregate_api_secrets WHERE aggregate_api_id = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query([api_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_aggregate_api_balance_secret(
        &self,
        api_id: &str,
        access_token: &str,
    ) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO aggregate_api_balance_secrets (aggregate_api_id, access_token, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(aggregate_api_id) DO UPDATE SET
               access_token = excluded.access_token,
               updated_at = excluded.updated_at",
            (api_id, access_token, now),
        )?;
        Ok(())
    }

    pub fn delete_aggregate_api_balance_secret(&self, api_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM aggregate_api_balance_secrets WHERE aggregate_api_id = ?1",
            [api_id],
        )?;
        Ok(())
    }

    pub fn find_aggregate_api_balance_secret_by_id(&self, api_id: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT access_token FROM aggregate_api_balance_secrets WHERE aggregate_api_id = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query([api_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// 函数 `update_aggregate_api_test_result`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    /// - ok: 参数 ok
    /// - status_code: 参数 status_code
    /// - error: 参数 error
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_aggregate_api_test_result(
        &self,
        api_id: &str,
        ok: bool,
        status_code: Option<i64>,
        error: Option<&str>,
    ) -> Result<()> {
        let now = now_ts();
        let last_test_status = if ok { Some("success") } else { Some("failed") };
        self.conn.execute(
            "UPDATE aggregate_apis
             SET last_test_at = ?1,
                 last_test_status = ?2,
                 last_test_error = ?3,
                 updated_at = ?1
             WHERE id = ?4",
            (now, last_test_status, error, api_id),
        )?;
        if let Some(code) = status_code {
            if !ok {
                let message = format!("http_status={code}");
                self.conn.execute(
                    "UPDATE aggregate_apis SET last_test_error = ?1 WHERE id = ?2",
                    (message, api_id),
                )?;
            }
        }
        Ok(())
    }

    /// 函数 `ensure_aggregate_apis_table`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - super: 参数 super
    ///
    /// # 返回
    /// 返回函数执行结果
    pub(super) fn ensure_aggregate_apis_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS aggregate_apis (
                id TEXT PRIMARY KEY,
                provider_type TEXT NOT NULL DEFAULT 'codex',
                supplier_name TEXT,
                sort INTEGER NOT NULL DEFAULT 0,
                url TEXT NOT NULL,
                auth_type TEXT NOT NULL DEFAULT 'apikey',
                auth_params_json TEXT,
                action TEXT,
                model_override TEXT,
                cost_multiplier REAL NOT NULL DEFAULT 1.0,
                daily_spend_limit_usd REAL,
                status TEXT NOT NULL DEFAULT 'active',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                last_test_at INTEGER,
                last_test_status TEXT,
                last_test_error TEXT,
                balance_query_enabled INTEGER NOT NULL DEFAULT 0,
                balance_query_template TEXT,
                balance_query_base_url TEXT,
                balance_query_user_id TEXT,
                balance_query_config_json TEXT,
                last_balance_at INTEGER,
                last_balance_status TEXT,
                last_balance_error TEXT,
                last_balance_json TEXT
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_aggregate_apis_created_at ON aggregate_apis(created_at DESC)",
            [],
        )?;
        self.ensure_column("aggregate_apis", "provider_type", "TEXT")?;
        self.ensure_column("aggregate_apis", "supplier_name", "TEXT")?;
        self.ensure_column("aggregate_apis", "sort", "INTEGER DEFAULT 0")?;
        self.ensure_column(
            "aggregate_apis",
            "auth_type",
            "TEXT NOT NULL DEFAULT 'apikey'",
        )?;
        self.ensure_column("aggregate_apis", "auth_params_json", "TEXT")?;
        self.ensure_column("aggregate_apis", "action", "TEXT")?;
        self.ensure_column("aggregate_apis", "model_override", "TEXT")?;
        self.ensure_column(
            "aggregate_apis",
            "cost_multiplier",
            "REAL NOT NULL DEFAULT 1.0",
        )?;
        self.ensure_column("aggregate_apis", "daily_spend_limit_usd", "REAL")?;
        self.ensure_column(
            "aggregate_apis",
            "balance_query_enabled",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.ensure_column("aggregate_apis", "balance_query_template", "TEXT")?;
        self.ensure_column("aggregate_apis", "balance_query_base_url", "TEXT")?;
        self.ensure_column("aggregate_apis", "balance_query_user_id", "TEXT")?;
        self.ensure_column("aggregate_apis", "balance_query_config_json", "TEXT")?;
        self.ensure_column("aggregate_apis", "last_balance_at", "INTEGER")?;
        self.ensure_column("aggregate_apis", "last_balance_status", "TEXT")?;
        self.ensure_column("aggregate_apis", "last_balance_error", "TEXT")?;
        self.ensure_column("aggregate_apis", "last_balance_json", "TEXT")?;
        self.conn.execute(
            "UPDATE aggregate_apis
             SET provider_type = COALESCE(NULLIF(TRIM(provider_type), ''), 'codex')
             WHERE provider_type IS NULL OR TRIM(provider_type) = ''",
            [],
        )?;
        self.conn.execute(
            "UPDATE aggregate_apis
             SET auth_type = COALESCE(NULLIF(TRIM(auth_type), ''), 'apikey')
             WHERE auth_type IS NULL OR TRIM(auth_type) = ''",
            [],
        )?;
        self.conn.execute(
            "UPDATE aggregate_apis
             SET sort = COALESCE(sort, 0)
             WHERE sort IS NULL",
            [],
        )?;
        self.conn.execute(
            "UPDATE aggregate_apis
             SET cost_multiplier = 1.0
             WHERE cost_multiplier IS NULL OR cost_multiplier <= 0",
            [],
        )?;
        self.conn.execute(
            "UPDATE aggregate_apis
             SET daily_spend_limit_usd = NULL
             WHERE daily_spend_limit_usd IS NOT NULL AND daily_spend_limit_usd <= 0",
            [],
        )?;
        Ok(())
    }

    /// 函数 `ensure_aggregate_api_secrets_table`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - super: 参数 super
    ///
    /// # 返回
    /// 返回函数执行结果
    pub(super) fn ensure_aggregate_api_secrets_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS aggregate_api_secrets (
                aggregate_api_id TEXT PRIMARY KEY REFERENCES aggregate_apis(id) ON DELETE CASCADE,
                secret_value TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_aggregate_api_secrets_updated_at ON aggregate_api_secrets(updated_at)",
            [],
        )?;
        Ok(())
    }

    pub(super) fn ensure_aggregate_api_balance_secrets_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS aggregate_api_balance_secrets (
                aggregate_api_id TEXT PRIMARY KEY REFERENCES aggregate_apis(id) ON DELETE CASCADE,
                access_token TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_aggregate_api_balance_secrets_updated_at ON aggregate_api_balance_secrets(updated_at)",
            [],
        )?;
        Ok(())
    }

    pub(super) fn ensure_aggregate_api_supplier_model_tables(&self) -> Result<()> {
        self.conn.execute_batch(include_str!(
            "../../migrations/059_aggregate_api_supplier_models.sql"
        ))
    }

    pub fn list_aggregate_api_supplier_models(
        &self,
        supplier_key: Option<&str>,
        provider_type: Option<&str>,
    ) -> Result<Vec<AggregateApiSupplierModel>> {
        let supplier_key = supplier_key
            .map(normalize_supplier_model_text)
            .filter(|value| !value.is_empty());
        let provider_type = provider_type
            .map(normalize_supplier_model_text)
            .filter(|value| !value.is_empty());
        let mut stmt = self.conn.prepare(
            "SELECT supplier_key, provider_type, upstream_model, display_name,
                    status, created_at, updated_at
             FROM aggregate_api_supplier_models
             ORDER BY supplier_key ASC, provider_type ASC, upstream_model ASC",
        )?;
        let rows = stmt.query_map([], map_aggregate_api_supplier_model_row)?;
        let mut items = Vec::new();
        for row in rows {
            let item = row?;
            if let Some(value) = supplier_key.as_deref() {
                if item.supplier_key != value {
                    continue;
                }
            }
            if let Some(value) = provider_type.as_deref() {
                if item.provider_type != value {
                    continue;
                }
            }
            items.push(item);
        }
        Ok(items)
    }

    pub fn upsert_aggregate_api_supplier_model(
        &self,
        model: &AggregateApiSupplierModel,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO aggregate_api_supplier_models (
                supplier_key, provider_type, upstream_model, display_name,
                status, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(supplier_key, provider_type, upstream_model) DO UPDATE SET
                display_name = excluded.display_name,
                status = excluded.status,
                updated_at = excluded.updated_at",
            params![
                &model.supplier_key,
                &model.provider_type,
                &model.upstream_model,
                &model.display_name,
                &model.status,
                model.created_at,
                model.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_aggregate_api_supplier_model(
        &self,
        supplier_key: &str,
        provider_type: &str,
        upstream_model: &str,
    ) -> Result<()> {
        self.conn.execute(
            "DELETE FROM aggregate_api_supplier_models
             WHERE supplier_key = ?1 AND provider_type = ?2 AND upstream_model = ?3",
            params![
                normalize_supplier_model_text(supplier_key),
                normalize_supplier_model_text(provider_type),
                normalize_supplier_model_text(upstream_model),
            ],
        )?;
        Ok(())
    }
}

/// 函数 `map_aggregate_api_row`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - row: 参数 row
///
/// # 返回
/// 返回函数执行结果
fn map_aggregate_api_row(row: &Row<'_>) -> Result<AggregateApi> {
    Ok(AggregateApi {
        id: row.get(0)?,
        provider_type: row.get(1)?,
        supplier_name: row.get(2)?,
        sort: row.get(3)?,
        url: row.get(4)?,
        auth_type: row.get(5)?,
        auth_params_json: row.get(6)?,
        action: row.get(7)?,
        model_override: row.get(8)?,
        cost_multiplier: row.get(9)?,
        daily_spend_limit_usd: row.get(10)?,
        status: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        last_test_at: row.get(14)?,
        last_test_status: row.get(15)?,
        last_test_error: row.get(16)?,
        balance_query_enabled: row.get(17)?,
        balance_query_template: row.get(18)?,
        balance_query_base_url: row.get(19)?,
        balance_query_user_id: row.get(20)?,
        balance_query_config_json: row.get(21)?,
        last_balance_at: row.get(22)?,
        last_balance_status: row.get(23)?,
        last_balance_error: row.get(24)?,
        last_balance_json: row.get(25)?,
    })
}

fn map_aggregate_api_supplier_model_row(row: &Row<'_>) -> Result<AggregateApiSupplierModel> {
    Ok(AggregateApiSupplierModel {
        supplier_key: row.get(0)?,
        provider_type: row.get(1)?,
        upstream_model: row.get(2)?,
        display_name: row.get(3)?,
        status: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn normalize_supplier_model_text(value: &str) -> String {
    value.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aggregate_api(id: &str, cost_multiplier: f64) -> AggregateApi {
        let now = now_ts();
        AggregateApi {
            id: id.to_string(),
            provider_type: "codex".to_string(),
            supplier_name: Some("Provider".to_string()),
            sort: 0,
            url: "https://api.example.com/v1".to_string(),
            auth_type: "apikey".to_string(),
            auth_params_json: None,
            action: None,
            model_override: None,
            cost_multiplier,
            daily_spend_limit_usd: None,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
            last_test_at: None,
            last_test_status: None,
            last_test_error: None,
            balance_query_enabled: false,
            balance_query_template: None,
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
    fn aggregate_api_cost_multiplier_round_trips() {
        let storage = Storage::open_in_memory().expect("open");
        storage.init().expect("init");

        storage
            .insert_aggregate_api(&aggregate_api("agg-cost", 1.75))
            .expect("insert aggregate api");

        let loaded = storage
            .find_aggregate_api_by_id("agg-cost")
            .expect("load aggregate api")
            .expect("aggregate api exists");

        assert!((loaded.cost_multiplier - 1.75).abs() < f64::EPSILON);
    }

    #[test]
    fn aggregate_api_cost_multiplier_can_be_updated() {
        let storage = Storage::open_in_memory().expect("open");
        storage.init().expect("init");
        storage
            .insert_aggregate_api(&aggregate_api("agg-cost-update", 1.0))
            .expect("insert aggregate api");

        storage
            .update_aggregate_api_cost_multiplier("agg-cost-update", 2.25)
            .expect("update multiplier");
        let loaded = storage
            .find_aggregate_api_by_id("agg-cost-update")
            .expect("load aggregate api")
            .expect("aggregate api exists");

        assert!((loaded.cost_multiplier - 2.25).abs() < f64::EPSILON);
    }

    #[test]
    fn aggregate_api_daily_spend_limit_round_trips_and_can_be_cleared() {
        let storage = Storage::open_in_memory().expect("open");
        storage.init().expect("init");
        let mut api = aggregate_api("agg-daily-limit", 1.0);
        api.daily_spend_limit_usd = Some(12.5);
        storage
            .insert_aggregate_api(&api)
            .expect("insert aggregate api");

        let loaded = storage
            .find_aggregate_api_by_id("agg-daily-limit")
            .expect("load aggregate api")
            .expect("aggregate api exists");
        assert_eq!(loaded.daily_spend_limit_usd, Some(12.5));

        storage
            .update_aggregate_api_daily_spend_limit_usd("agg-daily-limit", None)
            .expect("clear daily limit");
        let cleared = storage
            .find_aggregate_api_by_id("agg-daily-limit")
            .expect("reload aggregate api")
            .expect("aggregate api exists");
        assert_eq!(cleared.daily_spend_limit_usd, None);
    }
}

#[cfg(test)]
mod supplier_model_tests {
    use super::*;

    #[test]
    fn supplier_models_can_be_upserted_listed_and_deleted() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage
            .ensure_aggregate_api_supplier_model_tables()
            .expect("ensure tables");
        let now = now_ts();
        let model = AggregateApiSupplierModel {
            supplier_key: "test-supplier".to_string(),
            provider_type: "codex".to_string(),
            upstream_model: "provider-model".to_string(),
            display_name: Some("Provider Model".to_string()),
            status: "available".to_string(),
            created_at: now,
            updated_at: now,
        };

        storage
            .upsert_aggregate_api_supplier_model(&model)
            .expect("upsert model");
        let items = storage
            .list_aggregate_api_supplier_models(Some("test-supplier"), Some("codex"))
            .expect("list models");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].upstream_model, "provider-model");

        let mut disabled = model.clone();
        disabled.status = "disabled".to_string();
        disabled.updated_at = now + 1;
        storage
            .upsert_aggregate_api_supplier_model(&disabled)
            .expect("update model");
        let items = storage
            .list_aggregate_api_supplier_models(Some("test-supplier"), Some("codex"))
            .expect("list updated models");
        assert_eq!(items[0].status, "disabled");

        storage
            .delete_aggregate_api_supplier_model("test-supplier", "codex", "provider-model")
            .expect("delete model");
        let items = storage
            .list_aggregate_api_supplier_models(Some("test-supplier"), Some("codex"))
            .expect("list deleted models");
        assert!(items.is_empty());
    }
}

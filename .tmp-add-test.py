from pathlib import Path
path = Path(r'crates/core/src/storage/model_catalog_v2.rs')
text = path.read_text(encoding='utf-8')
if 'seed_backfills_quota_aggregate_routes_and_missing_grok_prices' in text:
    print('test already present')
    raise SystemExit(0)
marker = '''    fn migration_ignores_incomplete_legacy_route_schema() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage
            .conn
            .execute_batch(
                "CREATE TABLE schema_migrations (
                    version TEXT PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                 );
                 CREATE TABLE model_groups (id TEXT PRIMARY KEY);
                 CREATE TABLE request_logs (id INTEGER PRIMARY KEY);
                 CREATE TABLE model_source_mappings (id TEXT PRIMARY KEY);",
            )
            .expect("create partial legacy schema");

        storage
            .apply_model_catalog_v2_migration()
            .expect("migrate partial legacy schema");

        assert_eq!(
            storage
                .list_managed_models_v2(true)
                .expect("list migrated models")
                .len(),
            9
        );
    }
}
'''
if marker not in text:
    raise SystemExit('end marker not found')

new_tests = '''    fn migration_ignores_incomplete_legacy_route_schema() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage
            .conn
            .execute_batch(
                "CREATE TABLE schema_migrations (
                    version TEXT PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                 );
                 CREATE TABLE model_groups (id TEXT PRIMARY KEY);
                 CREATE TABLE request_logs (id INTEGER PRIMARY KEY);
                 CREATE TABLE model_source_mappings (id TEXT PRIMARY KEY);",
            )
            .expect("create partial legacy schema");

        storage
            .apply_model_catalog_v2_migration()
            .expect("migrate partial legacy schema");

        assert_eq!(
            storage
                .list_managed_models_v2(true)
                .expect("list migrated models")
                .len(),
            9
        );
    }

    #[test]
    fn seed_backfills_quota_aggregate_routes_and_missing_grok_prices() {
        let mut storage = storage();
        let now = now_ts();
        let api = AggregateApi {
            id: "agg-custom-1".to_string(),
            provider_type: "openai-compatible".to_string(),
            supplier_name: Some("custom-supplier".to_string()),
            sort: 0,
            url: "https://agg-custom-1.example.test".to_string(),
            auth_type: "bearer".to_string(),
            auth_params_json: None,
            action: None,
            model_override: None,
            cost_multiplier: 1.0,
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
        };
        storage
            .insert_aggregate_api(&api)
            .expect("insert aggregate api");

        let mut grok = storage
            .get_managed_model_v2("gpt-5.4")
            .expect("read template")
            .expect("template exists");
        grok.id.clear();
        grok.slug = "grok-4.5".to_string();
        grok.display_name = "Grok 4.5".to_string();
        grok.origin = "custom".to_string();
        grok.builtin_revision = None;
        grok.user_edited = true;
        grok.price = ModelPriceV2 {
            currency: "USD".to_string(),
            input_microusd_per_1m: None,
            cached_input_microusd_per_1m: None,
            output_microusd_per_1m: None,
            price_status: "missing".to_string(),
            price_source: None,
        };
        grok.price_tiers.clear();
        grok.routes.clear();
        storage
            .upsert_managed_model_v2(&ManagedModelV2Upsert {
                model: grok,
                ..Default::default()
            })
            .expect("insert custom grok");

        storage
            .set_quota_source_model_assignments(
                "aggregate_api",
                "agg-custom-1",
                &["gpt-5.6-sol".to_string(), "grok-4.5".to_string()],
            )
            .expect("set quota assignments");

        // Simulate an already-cutover DB that missed legacy route migration:
        // routes from seed remain, but aggregate routes from quota must still backfill.
        storage
            .seed_missing_builtin_models_v2()
            .expect("idempotent backfill");

        let sol = storage
            .get_managed_model_v2("gpt-5.6-sol")
            .expect("read sol")
            .expect("sol exists");
        assert!(
            sol.routes.iter().any(|route| {
                route.enabled
                    && route.source_kind == "aggregate_api"
                    && route.source_id == "agg-custom-1"
                    && route.upstream_model == "gpt-5.6-sol"
            }),
            "builtin model should receive aggregate route from quota assignment"
        );

        let grok = storage
            .get_managed_model_v2("grok-4.5")
            .expect("read grok")
            .expect("grok exists");
        assert_eq!(grok.price.price_status, "estimated");
        assert_eq!(
            grok.price.price_source.as_deref(),
            Some("xai_official_grok_4_5")
        );
        assert_eq!(grok.price.input_microusd_per_1m, Some(2_000_000));
        assert_eq!(grok.price.cached_input_microusd_per_1m, Some(500_000));
        assert_eq!(grok.price.output_microusd_per_1m, Some(6_000_000));
        assert_eq!(grok.price_tiers.len(), 2);
        assert_eq!(grok.price_tiers[0].min_input_tokens, 0);
        assert_eq!(grok.price_tiers[1].min_input_tokens, 200_000);
        assert_eq!(grok.price_tiers[1].input_microusd_per_1m, 4_000_000);
        assert!(
            grok.routes.iter().any(|route| {
                route.enabled
                    && route.source_kind == "aggregate_api"
                    && route.source_id == "agg-custom-1"
                    && route.upstream_model == "grok-4.5"
            }),
            "custom grok should receive aggregate route from quota assignment"
        );

        // second seed must remain idempotent and not overwrite user-edited prices
        storage
            .conn
            .execute(
                "UPDATE model_prices SET price_status='custom', price_source='user',
                   input_microusd_per_1m=111, cached_input_microusd_per_1m=222,
                   output_microusd_per_1m=333
                 WHERE model_id=(SELECT id FROM models WHERE slug='grok-4.5')",
                [],
            )
            .expect("mark user custom price");
        storage
            .seed_missing_builtin_models_v2()
            .expect("second backfill");
        let grok_after = storage
            .get_managed_model_v2("grok-4.5")
            .expect("read grok again")
            .expect("grok exists");
        assert_eq!(grok_after.price.price_status, "custom");
        assert_eq!(grok_after.price.input_microusd_per_1m, Some(111));
    }
}
'''
path.write_text(text.replace(marker, new_tests, 1), encoding='utf-8')
print('test added')

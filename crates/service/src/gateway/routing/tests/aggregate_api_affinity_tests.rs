#[cfg(test)]
mod aggregate_api_affinity_tests {
    use super::super::{
        derive_affinity_route_hash, reorder_candidates_with_affinity,
        should_apply_aggregate_api_affinity,
    };
    use codexmanager_core::storage::{AggregateApi, AggregateApiBinding, Storage};

    fn create_test_storage_with_affinity_enabled() -> Storage {
        let storage = Storage::open_in_memory().expect("failed to create in-memory storage");
        storage.init().expect("failed to initialize storage");
        // set_app_setting requires timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        storage
            .set_app_setting("aggregate_api_session_affinity_enabled", "1", now)
            .expect("failed to enable affinity");
        storage
    }

    fn create_test_binding(api_id: &str) -> AggregateApiBinding {
        AggregateApiBinding {
            platform_key_hash: "test-key-hash".to_string(),
            protocol_type: "openai_compat".to_string(),
            model: "gpt-5.6-terra".to_string(),
            cache_affinity_route_id_hash: "test-affinity-hash".to_string(),
            bound_aggregate_api_id: api_id.to_string(),
            bound_at: 1725350400,
            reason: Some("initial_success".to_string()),
        }
    }

    fn create_test_candidate(id: &str) -> AggregateApi {
        AggregateApi {
            id: id.to_string(),
            provider_type: "openai".to_string(),
            supplier_name: Some("test-supplier".to_string()),
            sort: 0,
            url: format!("https://{}.example.com", id),
            auth_type: "bearer".to_string(),
            auth_params_json: None,
            action: Some("active".to_string()),
            model_override: None,
            cost_multiplier: 1.0,
            daily_spend_limit_usd: None,
            status: "active".to_string(),
            created_at: 0,
            updated_at: 0,
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
            enable_consecutive_failure_freeze: true,
            upstream_protocol: None,
        }
    }

    #[test]
    fn should_apply_affinity_when_enabled_and_no_explicit_api() {
        let storage = create_test_storage_with_affinity_enabled();
        assert!(should_apply_aggregate_api_affinity(&storage, None));
    }

    #[test]
    fn should_not_apply_affinity_when_explicit_api_specified() {
        let storage = create_test_storage_with_affinity_enabled();
        assert!(!should_apply_aggregate_api_affinity(
            &storage,
            Some("explicit-api")
        ));
    }

    #[test]
    fn should_not_apply_affinity_when_disabled() {
        let storage = Storage::open_in_memory().expect("failed to create storage");
        storage.init().expect("failed to initialize storage");
        // affinity disabled by default
        assert!(!should_apply_aggregate_api_affinity(&storage, None));
    }

    #[test]
    fn derive_affinity_route_hash_is_deterministic() {
        let hash1 = derive_affinity_route_hash("test-route-id");
        let hash2 = derive_affinity_route_hash("test-route-id");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA256 produces 64 hex chars
    }

    #[test]
    fn derive_affinity_route_hash_differs_for_different_inputs() {
        let hash1 = derive_affinity_route_hash("route-a");
        let hash2 = derive_affinity_route_hash("route-b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn reorder_moves_bound_candidate_to_first() {
        let storage = create_test_storage_with_affinity_enabled();
        let binding = create_test_binding("agg-api-2");
        storage
            .upsert_aggregate_api_binding(&binding)
            .expect("upsert failed");

        let candidates = vec![
            create_test_candidate("agg-api-1"),
            create_test_candidate("agg-api-2"),
            create_test_candidate("agg-api-3"),
        ];

        let reordered = reorder_candidates_with_affinity(
            &storage,
            "test-key-hash",
            "openai_compat",
            Some("gpt-5.6-terra"),
            "test-affinity-hash",
            candidates,
        )
        .expect("reorder failed");

        assert_eq!(reordered.len(), 3);
        assert_eq!(reordered[0].id, "agg-api-2");
        assert_eq!(reordered[1].id, "agg-api-1");
        assert_eq!(reordered[2].id, "agg-api-3");
    }

    #[test]
    fn reorder_preserves_order_when_no_binding() {
        let storage = create_test_storage_with_affinity_enabled();
        // No binding created

        let candidates = vec![
            create_test_candidate("agg-api-1"),
            create_test_candidate("agg-api-2"),
        ];

        let reordered = reorder_candidates_with_affinity(
            &storage,
            "test-key-hash",
            "openai_compat",
            Some("gpt-5.6-terra"),
            "nonexistent-hash",
            candidates.clone(),
        )
        .expect("reorder failed");

        assert_eq!(reordered.len(), 2);
        assert_eq!(reordered[0].id, "agg-api-1");
        assert_eq!(reordered[1].id, "agg-api-2");
    }

    #[test]
    fn reorder_preserves_order_when_bound_candidate_not_in_list() {
        let storage = create_test_storage_with_affinity_enabled();
        let binding = create_test_binding("agg-api-unavailable");
        storage
            .upsert_aggregate_api_binding(&binding)
            .expect("upsert failed");

        let candidates = vec![
            create_test_candidate("agg-api-1"),
            create_test_candidate("agg-api-2"),
        ];

        let reordered = reorder_candidates_with_affinity(
            &storage,
            "test-key-hash",
            "openai_compat",
            Some("gpt-5.6-terra"),
            "test-affinity-hash",
            candidates.clone(),
        )
        .expect("reorder failed");

        assert_eq!(reordered.len(), 2);
        assert_eq!(reordered[0].id, "agg-api-1");
        assert_eq!(reordered[1].id, "agg-api-2");
    }
}

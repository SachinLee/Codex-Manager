#[cfg(test)]
mod aggregate_api_bindings_tests {
    use crate::storage::{AggregateApiBinding, Storage};

    fn create_test_storage() -> Storage {
        let storage = Storage::open_in_memory().expect("failed to create in-memory storage");
        storage.init().expect("failed to initialize storage");
        storage
    }
    fn create_test_binding(suffix: &str) -> AggregateApiBinding {
        AggregateApiBinding {
            platform_key_hash: format!("key-hash-{}", suffix),
            protocol_type: "openai_compat".to_string(),
            model: "gpt-5.6-terra".to_string(),
            cache_affinity_route_id_hash: format!("affinity-hash-{}", suffix),
            bound_aggregate_api_id: format!("agg-api-{}", suffix),
            bound_at: 1725350400 + suffix.parse::<i64>().unwrap_or(0),
            reason: Some("initial_success".to_string()),
        }
    }

    #[test]
    fn upsert_creates_new_binding() {
        let storage = create_test_storage();
        let binding = create_test_binding("1");

        storage
            .upsert_aggregate_api_binding(&binding)
            .expect("upsert failed");

        let retrieved = storage
            .get_aggregate_api_binding(
                &binding.platform_key_hash,
                &binding.protocol_type,
                &binding.model,
                &binding.cache_affinity_route_id_hash,
            )
            .expect("get failed");

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.bound_aggregate_api_id, "agg-api-1");
        assert_eq!(retrieved.reason, Some("initial_success".to_string()));
    }

    #[test]
    fn upsert_updates_existing_binding() {
        let storage = create_test_storage();
        let binding = create_test_binding("1");

        storage
            .upsert_aggregate_api_binding(&binding)
            .expect("initial upsert failed");

        let mut updated_binding = binding.clone();
        updated_binding.bound_aggregate_api_id = "agg-api-updated".to_string();
        updated_binding.bound_at = 1725360000;
        updated_binding.reason = Some("failover_convergence".to_string());

        storage
            .upsert_aggregate_api_binding(&updated_binding)
            .expect("update upsert failed");

        let retrieved = storage
            .get_aggregate_api_binding(
                &binding.platform_key_hash,
                &binding.protocol_type,
                &binding.model,
                &binding.cache_affinity_route_id_hash,
            )
            .expect("get failed");

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.bound_aggregate_api_id, "agg-api-updated");
        assert_eq!(retrieved.bound_at, 1725360000);
        assert_eq!(
            retrieved.reason,
            Some("failover_convergence".to_string())
        );
    }

    #[test]
    fn binding_partitioned_by_protocol() {
        let storage = create_test_storage();
        let binding1 = AggregateApiBinding {
            platform_key_hash: "key-hash-1".to_string(),
            protocol_type: "openai_compat".to_string(),
            model: "gpt-5.6-terra".to_string(),
            cache_affinity_route_id_hash: "affinity-hash-1".to_string(),
            bound_aggregate_api_id: "agg-api-openai".to_string(),
            bound_at: 1725350400,
            reason: None,
        };

        let binding2 = AggregateApiBinding {
            platform_key_hash: "key-hash-1".to_string(),
            protocol_type: "anthropic_native".to_string(),
            model: "gpt-5.6-terra".to_string(),
            cache_affinity_route_id_hash: "affinity-hash-1".to_string(),
            bound_aggregate_api_id: "agg-api-anthropic".to_string(),
            bound_at: 1725350400,
            reason: None,
        };

        storage
            .upsert_aggregate_api_binding(&binding1)
            .expect("upsert binding1 failed");
        storage
            .upsert_aggregate_api_binding(&binding2)
            .expect("upsert binding2 failed");

        let retrieved1 = storage
            .get_aggregate_api_binding(
                "key-hash-1",
                "openai_compat",
                "gpt-5.6-terra",
                "affinity-hash-1",
            )
            .expect("get binding1 failed")
            .unwrap();

        let retrieved2 = storage
            .get_aggregate_api_binding(
                "key-hash-1",
                "anthropic_native",
                "gpt-5.6-terra",
                "affinity-hash-1",
            )
            .expect("get binding2 failed")
            .unwrap();

        assert_eq!(retrieved1.bound_aggregate_api_id, "agg-api-openai");
        assert_eq!(retrieved2.bound_aggregate_api_id, "agg-api-anthropic");
    }

    #[test]
    fn binding_partitioned_by_model() {
        let storage = create_test_storage();
        let binding1 = AggregateApiBinding {
            platform_key_hash: "key-hash-1".to_string(),
            protocol_type: "openai_compat".to_string(),
            model: "gpt-5.6-terra".to_string(),
            cache_affinity_route_id_hash: "affinity-hash-1".to_string(),
            bound_aggregate_api_id: "agg-api-terra".to_string(),
            bound_at: 1725350400,
            reason: None,
        };

        let binding2 = AggregateApiBinding {
            platform_key_hash: "key-hash-1".to_string(),
            protocol_type: "openai_compat".to_string(),
            model: "gpt-5.4".to_string(),
            cache_affinity_route_id_hash: "affinity-hash-1".to_string(),
            bound_aggregate_api_id: "agg-api-54".to_string(),
            bound_at: 1725350400,
            reason: None,
        };

        storage
            .upsert_aggregate_api_binding(&binding1)
            .expect("upsert binding1 failed");
        storage
            .upsert_aggregate_api_binding(&binding2)
            .expect("upsert binding2 failed");

        let retrieved1 = storage
            .get_aggregate_api_binding(
                "key-hash-1",
                "openai_compat",
                "gpt-5.6-terra",
                "affinity-hash-1",
            )
            .expect("get binding1 failed")
            .unwrap();

        let retrieved2 = storage
            .get_aggregate_api_binding("key-hash-1", "openai_compat", "gpt-5.4", "affinity-hash-1")
            .expect("get binding2 failed")
            .unwrap();

        assert_eq!(retrieved1.bound_aggregate_api_id, "agg-api-terra");
        assert_eq!(retrieved2.bound_aggregate_api_id, "agg-api-54");
    }

    #[test]
    fn binding_partitioned_by_key_hash() {
        let storage = create_test_storage();
        let binding1 = AggregateApiBinding {
            platform_key_hash: "key-hash-1".to_string(),
            protocol_type: "openai_compat".to_string(),
            model: "gpt-5.6-terra".to_string(),
            cache_affinity_route_id_hash: "affinity-hash-1".to_string(),
            bound_aggregate_api_id: "agg-api-key1".to_string(),
            bound_at: 1725350400,
            reason: None,
        };

        let binding2 = AggregateApiBinding {
            platform_key_hash: "key-hash-2".to_string(),
            protocol_type: "openai_compat".to_string(),
            model: "gpt-5.6-terra".to_string(),
            cache_affinity_route_id_hash: "affinity-hash-1".to_string(),
            bound_aggregate_api_id: "agg-api-key2".to_string(),
            bound_at: 1725350400,
            reason: None,
        };

        storage
            .upsert_aggregate_api_binding(&binding1)
            .expect("upsert binding1 failed");
        storage
            .upsert_aggregate_api_binding(&binding2)
            .expect("upsert binding2 failed");

        let retrieved1 = storage
            .get_aggregate_api_binding(
                "key-hash-1",
                "openai_compat",
                "gpt-5.6-terra",
                "affinity-hash-1",
            )
            .expect("get binding1 failed")
            .unwrap();

        let retrieved2 = storage
            .get_aggregate_api_binding(
                "key-hash-2",
                "openai_compat",
                "gpt-5.6-terra",
                "affinity-hash-1",
            )
            .expect("get binding2 failed")
            .unwrap();

        assert_eq!(retrieved1.bound_aggregate_api_id, "agg-api-key1");
        assert_eq!(retrieved2.bound_aggregate_api_id, "agg-api-key2");
    }

    #[test]
    fn cleanup_deletes_stale_bindings() {
        let storage = create_test_storage();

        let old_binding = AggregateApiBinding {
            platform_key_hash: "key-hash-old".to_string(),
            protocol_type: "openai_compat".to_string(),
            model: "gpt-5.6-terra".to_string(),
            cache_affinity_route_id_hash: "affinity-hash-old".to_string(),
            bound_aggregate_api_id: "agg-api-old".to_string(),
            bound_at: 1725264000, // 24 hours ago
            reason: None,
        };

        let new_binding = AggregateApiBinding {
            platform_key_hash: "key-hash-new".to_string(),
            protocol_type: "openai_compat".to_string(),
            model: "gpt-5.6-terra".to_string(),
            cache_affinity_route_id_hash: "affinity-hash-new".to_string(),
            bound_aggregate_api_id: "agg-api-new".to_string(),
            bound_at: 1725350400, // recent
            reason: None,
        };

        storage
            .upsert_aggregate_api_binding(&old_binding)
            .expect("upsert old failed");
        storage
            .upsert_aggregate_api_binding(&new_binding)
            .expect("upsert new failed");

        let deleted = storage
            .delete_stale_aggregate_api_bindings(1725350000)
            .expect("cleanup failed");

        assert_eq!(deleted, 1);

        let old_retrieved = storage
            .get_aggregate_api_binding(
                "key-hash-old",
                "openai_compat",
                "gpt-5.6-terra",
                "affinity-hash-old",
            )
            .expect("get old failed");
        assert!(old_retrieved.is_none());

        let new_retrieved = storage
            .get_aggregate_api_binding(
                "key-hash-new",
                "openai_compat",
                "gpt-5.6-terra",
                "affinity-hash-new",
            )
            .expect("get new failed");
        assert!(new_retrieved.is_some());
    }

    #[test]
    fn get_nonexistent_binding_returns_none() {
        let storage = create_test_storage();

        let retrieved = storage
            .get_aggregate_api_binding(
                "nonexistent-key",
                "openai_compat",
                "gpt-5.6-terra",
                "nonexistent-affinity",
            )
            .expect("get failed");

        assert!(retrieved.is_none());
    }

    #[test]
    fn delete_removes_matching_binding_only() {
        let storage = create_test_storage();
        let binding = create_test_binding("1");
        let other = create_test_binding("2");

        storage
            .upsert_aggregate_api_binding(&binding)
            .expect("upsert failed");
        storage
            .upsert_aggregate_api_binding(&other)
            .expect("upsert failed");

        let deleted = storage
            .delete_aggregate_api_binding(
                &binding.platform_key_hash,
                &binding.protocol_type,
                &binding.model,
                &binding.cache_affinity_route_id_hash,
            )
            .expect("delete failed");
        assert_eq!(deleted, 1);

        assert!(
            storage
                .get_aggregate_api_binding(
                    &binding.platform_key_hash,
                    &binding.protocol_type,
                    &binding.model,
                    &binding.cache_affinity_route_id_hash,
                )
                .expect("get failed")
                .is_none()
        );
        assert!(
            storage
                .get_aggregate_api_binding(
                    &other.platform_key_hash,
                    &other.protocol_type,
                    &other.model,
                    &other.cache_affinity_route_id_hash,
                )
                .expect("get failed")
                .is_some()
        );
    }
}

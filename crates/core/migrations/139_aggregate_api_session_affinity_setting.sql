INSERT INTO app_settings (key, value, updated_at)
VALUES ('aggregate_api_session_affinity_enabled', '0', strftime('%s', 'now'))
ON CONFLICT (key) DO NOTHING;

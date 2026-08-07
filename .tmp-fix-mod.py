from pathlib import Path

p = Path('crates/core/src/storage/mod.rs')
text = p.read_text(encoding='utf-8')
old = '''<<<<<<< HEAD
        // The custom branch used colliding 113-116 migration numbers.  Close the
        // old schemas before the post-main bridge so its data repairs are valid
        // whether those historical SQL migrations were recorded or not.
        self.ensure_model_price_rules_table()?;
        self.ensure_request_token_stats_table()?;
        self.ensure_request_pricing_snapshots_table()?;
        self.apply_sql_migration(
            "117_custom_feature_bridge",
            include_str!("../../migrations/117_custom_feature_bridge.sql"),
=======
        self.apply_sql_or_compat_migration(
            "117_account_proxy_settings",
            include_str!("../../migrations/117_account_proxy_settings.sql"),
            |s| s.ensure_account_proxy_settings_table(),
        )?;
        self.apply_sql_or_compat_migration(
            "118_proxy_profiles",
            include_str!("../../migrations/118_proxy_profiles.sql"),
            |s| s.ensure_proxy_profiles_table(),
        )?;
        self.apply_sql_or_compat_migration(
            "119_proxy_profile_url_tests",
            include_str!("../../migrations/119_proxy_profile_url_tests.sql"),
            |s| s.ensure_proxy_profile_url_tests_table(),
        )?;
        self.apply_sql_or_compat_migration(
            "120_proxy_history",
            include_str!("../../migrations/120_proxy_history.sql"),
            |s| s.ensure_proxy_history_tables(),
        )?;
        self.apply_gpt56_official_pricing_migration()?;
        self.apply_sql_migration(
            "122_account_agent_identities",
            include_str!("../../migrations/122_account_agent_identities.sql"),
>>>>>>> origin/main
        )?;
'''
new = '''        self.apply_sql_or_compat_migration(
            "117_account_proxy_settings",
            include_str!("../../migrations/117_account_proxy_settings.sql"),
            |s| s.ensure_account_proxy_settings_table(),
        )?;
        self.apply_sql_or_compat_migration(
            "118_proxy_profiles",
            include_str!("../../migrations/118_proxy_profiles.sql"),
            |s| s.ensure_proxy_profiles_table(),
        )?;
        self.apply_sql_or_compat_migration(
            "119_proxy_profile_url_tests",
            include_str!("../../migrations/119_proxy_profile_url_tests.sql"),
            |s| s.ensure_proxy_profile_url_tests_table(),
        )?;
        self.apply_sql_or_compat_migration(
            "120_proxy_history",
            include_str!("../../migrations/120_proxy_history.sql"),
            |s| s.ensure_proxy_history_tables(),
        )?;
        self.apply_gpt56_official_pricing_migration()?;
        self.apply_sql_migration(
            "122_account_agent_identities",
            include_str!("../../migrations/122_account_agent_identities.sql"),
        )?;
        // Custom-feature bridge keeps its historical migration name so databases
        // that already applied it during the first integration continue cleanly.
        // Main's 117-122 use different names, so both chains coexist.
        self.ensure_model_price_rules_table()?;
        self.ensure_request_token_stats_table()?;
        self.ensure_request_pricing_snapshots_table()?;
        self.apply_sql_migration(
            "117_custom_feature_bridge",
            include_str!("../../migrations/117_custom_feature_bridge.sql"),
        )?;
'''
if old not in text:
    raise SystemExit('mod.rs conflict pattern missing')
p.write_text(text.replace(old, new), encoding='utf-8')
print('fixed mod.rs migrations')

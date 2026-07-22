mod import;
pub(crate) mod instructions;

use codexmanager_core::rpc::types::{
    ModelInfo, ModelReasoningLevel, ModelServiceTier, ModelTruncationPolicy, ModelsResponse,
};
use codexmanager_core::storage::{ManagedModelV2, ManagedModelV2Upsert, ModelCatalogV2Stats};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) use import::{
    commit_import, preview_import, ManagedModelImportCommitV2Params,
    ManagedModelImportPreviewV2Params, ManagedModelImportPreviewV2Result,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManagedModelListV2Result {
    pub items: Vec<ManagedModelV2>,
    pub stats: ModelCatalogV2Stats,
}

pub(crate) fn list(include_hidden: bool) -> Result<ManagedModelListV2Result, String> {
    let storage =
        crate::storage_helpers::open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    list_with_storage(&storage, include_hidden)
}

pub(crate) fn list_with_storage(
    storage: &codexmanager_core::storage::Storage,
    include_hidden: bool,
) -> Result<ManagedModelListV2Result, String> {
    Ok(ManagedModelListV2Result {
        items: storage
            .list_managed_models_v2(include_hidden)
            .map_err(|err| format!("list managed models V2 failed: {err}"))?,
        stats: storage
            .model_catalog_v2_stats()
            .map_err(|err| format!("read model catalog V2 stats failed: {err}"))?,
    })
}

pub(crate) fn get(slug: &str) -> Result<ManagedModelV2, String> {
    let storage =
        crate::storage_helpers::open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    storage
        .get_managed_model_v2(slug)
        .map_err(|err| format!("read managed model V2 failed: {err}"))?
        .ok_or_else(|| "model_not_found".to_string())
}

pub(crate) fn upsert(input: ManagedModelV2Upsert) -> Result<ManagedModelV2, String> {
    let storage =
        crate::storage_helpers::open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    storage
        .upsert_managed_model_v2(&input)
        .map_err(|err| format!("save managed model V2 failed: {err}"))
}

pub(crate) fn delete(slug: &str) -> Result<(), String> {
    let storage =
        crate::storage_helpers::open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    storage
        .delete_managed_model_v2(slug)
        .map_err(|err| format!("delete managed model V2 failed: {err}"))
}

fn capability<'a>(model: &'a ManagedModelV2, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| model.capabilities.get(*key))
}

pub(crate) fn model_info(model: &ManagedModelV2) -> ModelInfo {
    let string_list = |keys: &[&str]| {
        capability(model, keys)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let supported_reasoning_levels = string_list(&["reasoning_efforts", "reasoningEfforts"])
        .into_iter()
        .map(|effort| ModelReasoningLevel {
            effort,
            description: String::new(),
            ..Default::default()
        })
        .collect();
    let service_tiers = string_list(&["service_tiers", "serviceTiers"])
        .into_iter()
        .map(|id| ModelServiceTier {
            name: id.clone(),
            id,
            ..Default::default()
        })
        .collect();
    let truncation_policy = capability(model, &["truncation_mode", "truncationMode"])
        .and_then(Value::as_str)
        .zip(capability(model, &["truncation_limit", "truncationLimit"]).and_then(Value::as_i64))
        .map(|(mode, limit)| ModelTruncationPolicy {
            mode: mode.to_string(),
            limit,
            ..Default::default()
        });
    ModelInfo {
        slug: model.slug.clone(),
        display_name: model.display_name.clone(),
        description: model.description.clone(),
        default_reasoning_level: model.default_reasoning_effort.clone(),
        supported_reasoning_levels,
        visibility: Some(model.visibility.clone()),
        supported_in_api: model.supported_in_api,
        priority: model.sort_order,
        service_tiers,
        base_instructions: None,
        model_messages: None,
        supports_reasoning_summaries: capability(
            model,
            &["supports_reasoning_summaries", "supportsReasoningSummaries"],
        )
        .and_then(Value::as_bool),
        default_reasoning_summary: capability(
            model,
            &["default_reasoning_summary", "defaultReasoningSummary"],
        )
        .and_then(Value::as_str)
        .map(str::to_string),
        support_verbosity: capability(model, &["supports_verbosity", "supportsVerbosity"])
            .and_then(Value::as_bool),
        default_verbosity: capability(model, &["default_verbosity", "defaultVerbosity"]).cloned(),
        apply_patch_tool_type: capability(model, &["apply_patch_tool_type", "applyPatchToolType"])
            .and_then(Value::as_str)
            .map(str::to_string),
        web_search_tool_type: capability(model, &["web_search_tool_type", "webSearchToolType"])
            .and_then(Value::as_str)
            .map(str::to_string),
        truncation_policy,
        supports_parallel_tool_calls: capability(
            model,
            &["supports_parallel_tool_calls", "supportsParallelToolCalls"],
        )
        .and_then(Value::as_bool),
        supports_image_detail_original: capability(
            model,
            &[
                "supports_image_detail_original",
                "supportsImageDetailOriginal",
            ],
        )
        .and_then(Value::as_bool),
        context_window: model.context_window,
        input_modalities: string_list(&["input_modalities", "inputModalities"]),
        supports_search_tool: capability(model, &["supports_search_tool", "supportsSearchTool"])
            .and_then(Value::as_bool),
        ..Default::default()
    }
}

pub(crate) fn models_response_with_storage(
    storage: &codexmanager_core::storage::Storage,
) -> Result<ModelsResponse, String> {
    Ok(ModelsResponse {
        models: storage
            .list_api_models_v2()
            .map_err(|err| format!("list API models V2 failed: {err}"))?
            .iter()
            .map(model_info)
            .collect(),
        extra: Default::default(),
    })
}

import type { WebCommandDescriptor } from "./shared";
import { asRecord } from "./shared";

export function createAggregateApiWebCommands(): Record<string, WebCommandDescriptor> {
  return {
    service_aggregate_api_list: { rpcMethod: "aggregateApi/list" },
    service_aggregate_api_runtime_status_list: { rpcMethod: "aggregateApi/runtimeStatus/list" },
    service_aggregate_api_runtime_status_reset: { rpcMethod: "aggregateApi/runtimeStatus/reset" },
    service_aggregate_api_create: { rpcMethod: "aggregateApi/create" },
    service_aggregate_api_update: { rpcMethod: "aggregateApi/update" },
    service_aggregate_api_delete: { rpcMethod: "aggregateApi/delete" },
    service_aggregate_api_read_secret: { rpcMethod: "aggregateApi/readSecret" },
    service_aggregate_api_test_connection: { rpcMethod: "aggregateApi/testConnection" },
    service_aggregate_api_diagnose_capabilities: { rpcMethod: "aggregateApi/diagnoseCapabilities" },
    service_aggregate_api_capabilities_get: { rpcMethod: "aggregateApi/capabilities/get" },
    service_aggregate_api_capabilities_set_override: { rpcMethod: "aggregateApi/capabilities/setOverride" },
    service_aggregate_api_capabilities_reset_override: { rpcMethod: "aggregateApi/capabilities/resetOverride" },
    service_aggregate_api_capabilities_clear_observation: { rpcMethod: "aggregateApi/capabilities/clearObservation" },
    service_aggregate_api_capabilities_list_recent_attempts: { rpcMethod: "aggregateApi/capabilities/listRecentAttempts" },
    service_aggregate_api_capabilities_set_mode: { rpcMethod: "aggregateApi/capabilities/setMode" },
    service_aggregate_api_refresh_balance: { rpcMethod: "aggregateApi/refreshBalance" },
    service_aggregate_api_supplier_models_list: { rpcMethod: "aggregateApi/supplierModels/list" },
    service_aggregate_api_supplier_model_save: { rpcMethod: "aggregateApi/supplierModels/save", mapParams: (params) => asRecord(asRecord(params)?.payload) ?? {} },
    service_aggregate_api_supplier_model_delete: { rpcMethod: "aggregateApi/supplierModels/delete" },
    service_aggregate_api_supplier_models_import: { rpcMethod: "aggregateApi/sourceModels/importSupplier" },
  };
}

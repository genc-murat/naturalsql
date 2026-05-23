import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionStatus,
  SchemaResponse,
  NlToSqlRequest,
  SqlResponse,
  ExecuteRequest,
  QueryResult,
  LlmConfigResponse,
  UpdateLlmConfigRequest,
} from "./types";

export async function connectDb(connectionString: string): Promise<ConnectionStatus> {
  return invoke<ConnectionStatus>("connect_db", { connectionString });
}

export async function disconnectDb(): Promise<ConnectionStatus> {
  return invoke<ConnectionStatus>("disconnect_db");
}

export async function getConnectionStatus(): Promise<ConnectionStatus> {
  return invoke<ConnectionStatus>("get_connection_status");
}

export async function listDatabases(): Promise<string[]> {
  return invoke<string[]>("list_databases");
}

export async function cacheSchema(database: string): Promise<SchemaResponse> {
  return invoke<SchemaResponse>("cache_schema", { database });
}

export async function getCachedSchema(database: string): Promise<SchemaResponse> {
  return invoke<SchemaResponse>("get_cached_schema", { database });
}

export async function listCachedDatabases(): Promise<string[]> {
  return invoke<string[]>("list_cached_databases");
}

export async function removeCachedSchema(database: string): Promise<void> {
  return invoke("remove_cached_schema", { database });
}

export async function nlToSql(request: NlToSqlRequest): Promise<SqlResponse> {
  return invoke<SqlResponse>("nl_to_sql", { request });
}

export async function executeSql(request: ExecuteRequest): Promise<QueryResult> {
  return invoke<QueryResult>("execute_sql", { request });
}

export async function getLlmConfig(): Promise<LlmConfigResponse> {
  return invoke<LlmConfigResponse>("get_llm_config");
}

export async function updateLlmConfig(request: UpdateLlmConfigRequest): Promise<LlmConfigResponse> {
  return invoke<LlmConfigResponse>("update_llm_config", { request });
}

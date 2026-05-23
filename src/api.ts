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
  ConnectionProfileResponse,
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

export async function explainSql(request: ExecuteRequest): Promise<QueryResult> {
  return invoke<QueryResult>("explain_sql", { request });
}

export async function explainSqlNatural(request: ExecuteRequest): Promise<{ explanation: string }> {
  return invoke<{ explanation: string }>("explain_sql_natural", { request });
}

export async function fixSql(request: { sql: string; error: string }): Promise<{ fixed_sql: string; explanation: string }> {
  return invoke<{ fixed_sql: string; explanation: string }>("fix_sql", { request });
}

export async function optimizeSql(request: { sql: string }): Promise<{ original_explain: string; suggestions: string; optimized_sql: string | null }> {
  return invoke<{ original_explain: string; suggestions: string; optimized_sql: string | null }>("optimize_sql", { request });
}

export async function buildJoin(request: { description: string }): Promise<{ sql: string }> {
  return invoke<{ sql: string }>("build_join", { request });
}

export async function analyzeData(request: { question: string }): Promise<{ sql: string; answer: string; data: { columns: string[]; rows: unknown[][]; row_count: number } | null }> {
  return invoke<{ sql: string; answer: string; data: { columns: string[]; rows: unknown[][]; row_count: number } | null }>("analyze_data", { request });
}

export async function getLlmConfig(): Promise<LlmConfigResponse> {
  return invoke<LlmConfigResponse>("get_llm_config");
}

export async function updateLlmConfig(request: UpdateLlmConfigRequest): Promise<LlmConfigResponse> {
  return invoke<LlmConfigResponse>("update_llm_config", { request });
}

export async function listConnections(): Promise<ConnectionProfileResponse[]> {
  return invoke<ConnectionProfileResponse[]>("list_connections");
}

export async function saveConnectionProfile(profile: ConnectionProfileResponse): Promise<void> {
  return invoke("save_connection_profile", { profile });
}

export async function deleteConnectionProfile(name: string): Promise<void> {
  return invoke("delete_connection_profile", { name });
}

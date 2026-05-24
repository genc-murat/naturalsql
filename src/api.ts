import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionStatus,
  SchemaResponse,
  NlToSqlRequest,
  NlToSqlResponse,
  ExecuteRequest,
  QueryResult,
  LlmConfigResponse,
  UpdateLlmConfigRequest,
  ConnectionProfileResponse,
  TableStructure,
  SchemaMigrationResponse,
  DataEditResponse,
  Dashboard,
  ErDiagramResponse,
} from "./types";
import { listen } from "@tauri-apps/api/event";

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

export async function nlToSql(request: NlToSqlRequest): Promise<NlToSqlResponse> {
  return invoke<NlToSqlResponse>("nl_to_sql", { request });
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

export async function validateCrossDbJoin(request: { left_table: string; right_table: string }): Promise<{ 
  valid: boolean; 
  left_exists: boolean; 
  right_exists: boolean; 
  suggested_join_columns: [string, string][];
  has_relationship: boolean;
}> {
  return invoke<{ 
    valid: boolean; 
    left_exists: boolean; 
    right_exists: boolean; 
    suggested_join_columns: [string, string][];
    has_relationship: boolean;
  }>("validate_cross_db_join", { request });
}

export async function analyzeData(request: { question: string }): Promise<{ sql: string; answer: string; data: { columns: string[]; rows: unknown[][]; row_count: number } | null; dashboard?: Dashboard }> {
  return invoke<{ sql: string; answer: string; data: { columns: string[]; rows: unknown[][]; row_count: number } | null; dashboard?: Dashboard }>("analyze_data", { request });
}

export async function resultSetAction(request: { question: string; columns: string[]; sample_rows: unknown[][]; total_rows: number }): Promise<{ response: string; suggested_sql: string | null }> {
  return invoke<{ response: string; suggested_sql: string | null }>("result_set_action", { request });
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

export async function getTableStructure(database: string, table: string): Promise<TableStructure> {
  return invoke<TableStructure>("get_table_structure", { request: { database, table } });
}

export async function explainSqlJson(sql: string): Promise<string> {
  return invoke<string>("explain_sql_json", { request: { sql } });
}

export async function schemaMigration(naturalLanguage: string, database: string): Promise<SchemaMigrationResponse> {
  return invoke<SchemaMigrationResponse>("schema_migration", { request: { natural_language: naturalLanguage, database } });
}

export async function nlDataEdit(naturalLanguage: string, database: string): Promise<DataEditResponse> {
  return invoke<DataEditResponse>("nl_data_edit", { request: { natural_language: naturalLanguage, database } });
}

export async function executeSqlStreaming(sql: string, queryId: string): Promise<void> {
  return invoke<void>("execute_sql_streaming", { request: { sql, query_id: queryId } });
}

export async function getErDiagramData(database: string): Promise<ErDiagramResponse> {
  return invoke<ErDiagramResponse>("get_er_diagram_data", { request: { database } });
}

export async function cancelRunningQuery(queryId: string): Promise<boolean> {
  return invoke<boolean>("cancel_running_query", { queryId });
}

let unlistenBatchFn: (() => void) | null = null;
let unlistenDoneFn: (() => void) | null = null;
let unlistenErrorFn: (() => void) | null = null;

export async function setupStreamListeners(
  queryId: string,
  onBatch: (columns: string[], rows: unknown[][], totalSoFar: number) => void,
  onDone: (totalSoFar: number) => void,
  onError: (error: string) => void,
): Promise<void> {
  cleanupStreamListeners();

  const batch = await listen<{ query_id: string; columns: string[]; rows: unknown[][]; total_so_far: number }>("sql-stream-batch", (event) => {
    if (event.payload.query_id === queryId) {
      onBatch(event.payload.columns, event.payload.rows, event.payload.total_so_far);
    }
  });
  unlistenBatchFn = batch;

  const done = await listen<{ query_id: string; total_so_far: number }>("sql-stream-done", (event) => {
    if (event.payload.query_id === queryId) {
      onDone(event.payload.total_so_far);
    }
  });
  unlistenDoneFn = done;

  const err = await listen<{ query_id: string; error: string }>("sql-stream-error", (event) => {
    if (event.payload.query_id === queryId) {
      onError(event.payload.error);
    }
  });
  unlistenErrorFn = err;
}

export function cleanupStreamListeners() {
  unlistenBatchFn?.();
  unlistenDoneFn?.();
  unlistenErrorFn?.();
  unlistenBatchFn = null;
  unlistenDoneFn = null;
  unlistenErrorFn = null;
}

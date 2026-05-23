import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionStatus,
  SchemaResponse,
  NlToSqlRequest,
  SqlResponse,
  ExecuteRequest,
  QueryResult,
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

export async function cacheSchema(connectionString: string): Promise<SchemaResponse> {
  return invoke<SchemaResponse>("cache_schema", { connectionString });
}

export async function getCachedSchema(): Promise<SchemaResponse> {
  return invoke<SchemaResponse>("get_cached_schema");
}

export async function nlToSql(request: NlToSqlRequest): Promise<SqlResponse> {
  return invoke<SqlResponse>("nl_to_sql", { request });
}

export async function executeSql(request: ExecuteRequest): Promise<QueryResult> {
  return invoke<QueryResult>("execute_sql", { request });
}

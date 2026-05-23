// Types for Tauri commands
export interface ConnectionStatus {
  connected: boolean;
}

export interface ColumnInfo {
  name: string;
  column_type: string;
  is_nullable: boolean;
  column_key: string;
}

export interface TableInfo {
  name: string;
  columns: ColumnInfo[];
}

export interface Schema {
  database: string;
  tables: TableInfo[];
}

export interface SchemaResponse {
  schema: Schema | null;
}

export interface NlToSqlRequest {
  natural_language: string;
  database: string;
}

export interface SqlResponse {
  sql: string;
}

export interface ExecuteRequest {
  sql: string;
}

export interface QueryResult {
  columns: string[];
  rows: (string | number | boolean | null)[][];
  row_count: number;
  execution_time_ms: number;
  affected_rows: number | null;
}

export interface LlmConfigResponse {
  url: string;
  model: string;
}

export interface ConnectionProfileResponse {
  name: string;
  host: string;
  port: string;
  user: string;
  password: string;
  database: string;
}

export interface UpdateLlmConfigRequest {
  url: string;
  model: string;
}

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

export interface ToolCallStep {
  tool_name: string;
  parameters: Record<string, string>;
  result: string;
  iteration: number;
}

export interface NlToSqlResponse {
  sql: string;
  tool_calls: ToolCallStep[];
  iterations: number;
  used_fallback: boolean;
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

export interface IndexInfo {
  name: string;
  column: string;
  non_unique: boolean;
  seq: number;
  index_type: string;
}

export interface ConstraintInfo {
  name: string;
  column: string;
  constraint_type: string;
}

export interface ForeignKeyRelation {
  from_database: string;
  from_table: string;
  from_column: string;
  to_database: string;
  to_table: string;
  to_column: string;
  constraint_name: string | null;
}

export interface TableStats {
  row_count: number;
  data_size_mb: number;
  index_size_mb: number;
  avg_row_length: number;
}

export interface TableStatus {
  engine: string;
  row_format: string;
  collation: string;
  auto_increment: number | null;
  create_time: string | null;
  update_time: string | null;
}

export interface TableStructure {
  ddl: string;
  indexes: IndexInfo[];
  constraints: ConstraintInfo[];
  foreign_keys: ForeignKeyRelation[];
  stats: TableStats;
  status: TableStatus;
}

export interface QueryTab {
  id: string;
  name: string;
  sql: string;
  naturalLanguage: string;
  result: QueryResult | null;
  toolSteps: ToolCallStep[];
  toolIterations: number;
  toolFallback: boolean;
}

export interface SessionState {
  tabs: Array<{ id: string; name: string; sql: string; naturalLanguage: string }>;
  activeTabId: string;
  selectedDatabase: string | null;
  sidebarWidth: number;
  isSidebarCollapsed: boolean;
  connectionProfileName: string | null;
  lastSaved: number;
}

export interface SchemaMigrationResponse {
  sql: string;
  explanation: string;
  risk_level: string;
}

export interface DataEditResponse {
  sql: string;
  preview_sql: string;
  explanation: string;
  undo_sql: string;
  affected_estimate: number;
}

export interface StreamBatch {
  query_id: string;
  columns: string[];
  rows: (string | number | boolean | null)[][];
  total_so_far: number;
  done: boolean;
}

export interface StreamState {
  queryId: string;
  isStreaming: boolean;
  columns: string[];
  rows: (string | number | boolean | null)[][];
  totalSoFar: number;
  error: string | null;
}

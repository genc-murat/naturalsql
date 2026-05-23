import { useState } from "react";
import { ChevronRight, ChevronDown, Table2, Key, Hash, Type, Calendar, Database, Download, Loader2, CheckCircle } from "lucide-react";
import type { Schema } from "../types";

interface SchemaBrowserProps {
  schema: Schema | null;
  databases: string[];
  cachedDatabases: string[];
  selectedDatabase: string | null;
  onSelectDatabase: (db: string) => void;
  onCacheDatabase: (db: string) => void;
  onClearCache: (db: string) => void;
  isCaching: boolean;
  cachingDatabase: string | null;
}

function getColumnIcon(columnType: string) {
  const lower = columnType.toLowerCase();
  if (lower.includes("int") || lower.includes("float") || lower.includes("decimal") || lower.includes("double")) {
    return <Hash className="w-3.5 h-3.5 text-blue-400" />;
  }
  if (lower.includes("date") || lower.includes("time") || lower.includes("year")) {
    return <Calendar className="w-3.5 h-3.5 text-green-400" />;
  }
  if (lower.includes("text") || lower.includes("char") || lower.includes("blob") || lower.includes("enum")) {
    return <Type className="w-3.5 h-3.5 text-purple-400" />;
  }
  return <Hash className="w-3.5 h-3.5 text-gray-400" />;
}

function TableNode({ table }: { table: Schema["tables"][number] }) {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div>
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="w-full flex items-center gap-2 px-2 py-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-left transition-colors group"
      >
        {isExpanded ? (
          <ChevronDown className="w-4 h-4 text-[var(--text-muted)]" />
        ) : (
          <ChevronRight className="w-4 h-4 text-[var(--text-muted)]" />
        )}
        <Table2 className="w-4 h-4 text-[var(--accent)]" />
        <span className="text-sm font-medium text-[var(--text-primary)] truncate">
          {table.name}
        </span>
        <span className="text-xs text-[var(--text-muted)] ml-auto">
          {table.columns.length}
        </span>
      </button>
      {isExpanded && (
        <div className="ml-6 pl-3 border-l border-[var(--border)]">
          {table.columns.map((col) => (
            <div
              key={col.name}
              className="flex items-center gap-2 px-2 py-1 text-xs group"
            >
              {getColumnIcon(col.column_type)}
              <span className="text-[var(--text-primary)] flex-1 truncate">
                {col.name}
              </span>
              <span className="text-[var(--text-muted)] opacity-0 group-hover:opacity-100 transition-opacity">
                {col.column_type.split("(")[0]}
              </span>
              {col.column_key === "PRI" && (
                <span title="Primary Key">
                  <Key className="w-3 h-3 text-yellow-500" />
                </span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export function SchemaBrowser({
  schema,
  databases,
  cachedDatabases,
  selectedDatabase,
  onSelectDatabase,
  onCacheDatabase,
  onClearCache,
  isCaching,
  cachingDatabase,
}: SchemaBrowserProps) {
  if (databases.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-[var(--text-muted)] text-sm">
        Connect to see databases
      </div>
    );
  }

  return (
    <div className="space-y-1">
      {databases.map((db) => {
        const isCached = cachedDatabases.includes(db);
        const isSelected = selectedDatabase === db;
        const isCachingThis = isCaching && cachingDatabase === db;

        return (
          <div key={db}>
            {/* Database row */}
            <div
              className={`flex items-center gap-1.5 px-2 py-1.5 rounded-md cursor-pointer transition-colors ${
                isSelected
                  ? "bg-[var(--accent)]/10 text-[var(--accent)]"
                  : "hover:bg-[var(--bg-tertiary)] text-[var(--text-secondary)]"
              }`}
              onClick={() => onSelectDatabase(db)}
            >
              <Database className="w-4 h-4 flex-shrink-0" />
              <span className="text-sm font-medium truncate flex-1">{db}</span>
              {isCached && !isCachingThis && (
                <CheckCircle className="w-3.5 h-3.5 text-[var(--success)] flex-shrink-0" />
              )}
              {isCached && (
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onClearCache(db);
                  }}
                  className="p-0.5 rounded hover:bg-[var(--bg-tertiary)] transition-colors"
                  title={`Clear cached schema for ${db}`}
                >
                  <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="w-3.5 h-3.5 text-[var(--text-muted)] hover:text-[var(--error)]"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>
                </button>
              )}
              {isCachingThis && (
                <Loader2 className="w-3.5 h-3.5 animate-spin text-[var(--accent)] flex-shrink-0" />
              )}
              {!isCached && !isCachingThis && (
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onCacheDatabase(db);
                  }}
                  className="p-0.5 rounded hover:bg-[var(--bg-tertiary)] transition-colors"
                  title={`Cache schema for ${db}`}
                >
                  <Download className="w-3.5 h-3.5 text-[var(--text-muted)]" />
                </button>
              )}
            </div>

            {/* Schema tables if selected and cached */}
            {isSelected && schema && (
              <div className="ml-4 pl-2 border-l border-[var(--border)]">
                <div className="px-2 py-1 text-xs text-[var(--text-muted)]">
                  {schema.tables.length} table{schema.tables.length !== 1 ? "s" : ""}
                </div>
                {schema.tables.map((table) => (
                  <TableNode key={table.name} table={table} />
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

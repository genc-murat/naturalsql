import { useState, useMemo } from "react";
import { ChevronRight, ChevronDown, Table2, Key, Hash, Type, Calendar, Database, Download, Loader2, CheckCircle, Eye, Search, X } from "lucide-react";
import type { Schema } from "../types";

interface SchemaBrowserProps {
  schema: Schema | null;
  databases: string[];
  cachedDatabases: string[];
  selectedDatabase: string | null;
  onSelectDatabase: (db: string) => void;
  onCacheDatabase: (db: string) => void;
  onClearCache: (db: string) => void;
  onViewData: (database: string, table: string) => void;
  onViewStructure?: (database: string, table: string) => void;
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

function HighlightText({ text, query }: { text: string; query: string }) {
  if (!query.trim()) return <>{text}</>;
  const q = query.toLowerCase();
  const idx = text.toLowerCase().indexOf(q);
  if (idx === -1) return <>{text}</>;
  return (
    <>
      {text.slice(0, idx)}
      <mark className="bg-[var(--accent)]/30 text-[var(--text-primary)] rounded-sm px-0.5">
        {text.slice(idx, idx + query.length)}
      </mark>
      {text.slice(idx + query.length)}
    </>
  );
}

function TableNode({ table, database, onViewData, onViewStructure, searchQuery = "" }: { table: Schema["tables"][number]; database: string; onViewData: (database: string, table: string) => void; onViewStructure?: (database: string, table: string) => void; searchQuery?: string }) {
  const [isExpanded, setIsExpanded] = useState(false);
  const [showContext, setShowContext] = useState<{ x: number; y: number } | null>(null);

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    setShowContext({ x: e.clientX, y: e.clientY });
  };

  return (
    <div>
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        onContextMenu={handleContextMenu}
        className="w-full flex items-center gap-2 px-2 py-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-left transition-colors group"
      >
        {isExpanded ? (
          <ChevronDown className="w-4 h-4 text-[var(--text-muted)]" />
        ) : (
          <ChevronRight className="w-4 h-4 text-[var(--text-muted)]" />
        )}
        <Table2 className="w-4 h-4 text-[var(--accent)]" />
        <span className="text-sm font-medium text-[var(--text-primary)] truncate">
          <HighlightText text={table.name} query={searchQuery} />
        </span>
        <span className="text-xs text-[var(--text-muted)] ml-auto">
          {table.columns.length}
        </span>
        <Eye className="w-3 h-3 text-[var(--text-muted)] opacity-0 group-hover:opacity-100 transition-opacity ml-auto" />
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
                <HighlightText text={col.name} query={searchQuery} />
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

      {/* Context Menu */}
      {showContext && (
        <div
          className="fixed z-50 min-w-[160px] rounded-lg bg-[var(--bg-primary)] border border-[var(--border)] shadow-xl py-1"
          style={{ top: showContext.y, left: showContext.x }}
          onClick={() => setShowContext(null)}
        >
          <button
            onClick={() => {
              onViewData(database, table.name);
              setShowContext(null);
            }}
            className="w-full px-3 py-2 text-sm text-left hover:bg-[var(--bg-secondary)] flex items-center gap-2 text-[var(--text-primary)]"
          >
            <Eye className="w-3.5 h-3.5 text-[var(--accent)]" />
            View Data
          </button>
          {onViewStructure && (
            <button
              onClick={() => {
                onViewStructure(database, table.name);
                setShowContext(null);
              }}
              className="w-full px-3 py-2 text-sm text-left hover:bg-[var(--bg-secondary)] flex items-center gap-2 text-[var(--text-primary)]"
            >
              <Table2 className="w-3.5 h-3.5 text-[var(--accent)]" />
              View Structure
            </button>
          )}
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
  onViewData,
  isCaching,
  cachingDatabase,
}: SchemaBrowserProps) {
  const [searchQuery, setSearchQuery] = useState("");

  const filteredSchema = useMemo(() => {
    if (!schema || !searchQuery.trim()) return schema;
    const q = searchQuery.toLowerCase().trim();
    return {
      ...schema,
      tables: schema.tables
        .map((t) => {
          const tableMatch = t.name.toLowerCase().includes(q);
          if (tableMatch) return t;
          const matchedCols = t.columns.filter((c) => c.name.toLowerCase().includes(q));
          if (matchedCols.length > 0) return { ...t, columns: matchedCols };
          return null;
        })
        .filter(Boolean) as Schema["tables"],
    };
  }, [schema, searchQuery]);

  if (databases.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-[var(--text-muted)] text-sm">
        Connect to see databases
      </div>
    );
  }

  return (
    <div className="space-y-1">
      {selectedDatabase && schema && (
        <div className="px-1 pb-1">
          <div className="relative">
            <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-[var(--text-muted)]" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Filter tables & columns..."
              className="w-full pl-7 pr-7 py-1.5 rounded-md bg-[var(--bg-tertiary)] border border-[var(--border)] text-sm text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
            />
            {searchQuery && (
              <button
                onClick={() => setSearchQuery("")}
                className="absolute right-1.5 top-1/2 -translate-y-1/2 p-0.5 rounded hover:bg-[var(--bg-secondary)]"
              >
                <X className="w-3.5 h-3.5 text-[var(--text-muted)]" />
              </button>
            )}
          </div>
          {searchQuery.trim() && filteredSchema && (
            <div className="text-xs text-[var(--text-muted)] mt-1 px-1">
              {filteredSchema.tables.length}/{schema.tables.length} tables
            </div>
          )}
        </div>
      )}
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
            {isSelected && filteredSchema && (
              <div className="ml-4 pl-2 border-l border-[var(--border)]">
                <div className="px-2 py-1 text-xs text-[var(--text-muted)]">
                  {filteredSchema.tables.length} table{filteredSchema.tables.length !== 1 ? "s" : ""}
                </div>
                {filteredSchema.tables.map((table) => (
                  <TableNode key={table.name} table={table} database={selectedDatabase!} onViewData={onViewData} searchQuery={searchQuery} />
                ))}
                {filteredSchema.tables.length === 0 && searchQuery.trim() && (
                  <div className="px-2 py-3 text-xs text-[var(--text-muted)] text-center">
                    No tables match &ldquo;{searchQuery}&rdquo;
                  </div>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

import { useState } from "react";
import { ChevronRight, ChevronDown, Table2, Key, Hash, Type, Calendar } from "lucide-react";
import type { Schema } from "../types";

interface SchemaBrowserProps {
  schema: Schema | null;
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

export function SchemaBrowser({ schema }: SchemaBrowserProps) {
  if (!schema) {
    return (
      <div className="flex items-center justify-center h-full text-[var(--text-muted)] text-sm">
        Connect and cache schema to browse
      </div>
    );
  }

  return (
    <div className="space-y-1">
      <div className="px-2 py-1.5 text-xs font-semibold text-[var(--text-muted)] uppercase tracking-wider">
        {schema.database} ({schema.tables.length} tables)
      </div>
      {schema.tables.map((table) => (
        <TableNode key={table.name} table={table} />
      ))}
    </div>
  );
}

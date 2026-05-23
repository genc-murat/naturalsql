import { useState, useRef, useEffect } from "react";
import {
  GitBranch,
  X,
  Loader2,
  ArrowRight,
  Send,
  Database,
} from "lucide-react";
import { buildJoin, listCachedDatabases } from "../api";

interface JoinBuilderProps {
  isOpen: boolean;
  onClose: () => void;
  onApply: (sql: string) => void;
  tableNames: string[];
}

interface TableWithDatabase {
  database: string;
  table: string;
  fullName: string;
}

const JOIN_TYPES = [
  { value: "INNER JOIN", label: "INNER JOIN", desc: "Matching rows only" },
  { value: "LEFT JOIN", label: "LEFT JOIN", desc: "All left + matching right" },
  { value: "RIGHT JOIN", label: "RIGHT JOIN", desc: "All right + matching left" },
  { value: "CROSS JOIN", label: "CROSS JOIN", desc: "Cartesian product" },
];

export function JoinBuilder({ isOpen, onClose, onApply, tableNames }: JoinBuilderProps) {
  const [joinType, setJoinType] = useState("INNER JOIN");
  const [leftTable, setLeftTable] = useState("");
  const [rightTable, setRightTable] = useState("");
  const [leftColumn, setLeftColumn] = useState("");
  const [rightColumn, setRightColumn] = useState("");
  const [where, setWhere] = useState("");
  const [columns, setColumns] = useState("*");
  const [description, setDescription] = useState("");
  const [isBuilding, setIsBuilding] = useState(false);
  const [error, setError] = useState("");
  const [mode, setMode] = useState<"manual" | "ai">("manual");
  const [availableTables, setAvailableTables] = useState<TableWithDatabase[]>([]);
  const [crossDbMode, setCrossDbMode] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isOpen) {
      loadTables();
    }
  }, [isOpen, tableNames]);

  useEffect(() => {
    if (isOpen && inputRef.current) {
      inputRef.current.focus();
    }
  }, [isOpen]);

  const loadTables = async () => {
    try {
      // Try to get databases from cache
      const databases = await listCachedDatabases();
      
      if (databases.length > 1) {
        // Multiple databases - enable cross-database mode
        setCrossDbMode(true);
        const tables: TableWithDatabase[] = [];
        
        // For now, use tableNames as-is (should already have database.table format)
        // In a real implementation, you'd fetch schema for each database
        for (const db of databases) {
          for (const table of tableNames) {
            if (!table.includes('.')) {
              tables.push({
                database: db,
                table,
                fullName: `${db}.${table}`,
              });
            }
          }
        }
        
        // Also include any tables that already have database prefix
        for (const table of tableNames) {
          if (table.includes('.')) {
            const [db, tbl] = table.split('.');
            tables.push({
              database: db,
              table: tbl,
              fullName: table,
            });
          }
        }
        
        setAvailableTables(tables);
      } else {
        // Single database mode
        setCrossDbMode(false);
        const tables: TableWithDatabase[] = tableNames.map(t => {
          if (t.includes('.')) {
            const [db, tbl] = t.split('.');
            return { database: db, table: tbl, fullName: t };
          }
          return { database: databases[0] || 'unknown', table: t, fullName: t };
        });
        setAvailableTables(tables);
      }
    } catch (err) {
      console.error('Failed to load tables:', err);
      // Fallback to tableNames as-is
      setAvailableTables(tableNames.map(t => ({
        database: '',
        table: t,
        fullName: t,
      })));
    }
  };

  const generateManualSql = () => {
    if (!leftTable || !rightTable) {
      setError("Select both tables");
      return;
    }
    const cols = columns.trim() || "*";
    let sql = `SELECT ${cols}\nFROM ${leftTable}\n${joinType} ${rightTable}`;
    if (leftColumn && rightColumn) {
      sql += `\n  ON ${leftTable}.${leftColumn} = ${rightTable}.${rightColumn}`;
    }
    if (where.trim()) {
      sql += `\nWHERE ${where.trim()}`;
    }
    sql += ";";
    return sql;
  };

  const handleGenerate = () => {
    if (mode === "manual") {
      const sql = generateManualSql();
      if (sql) {
        onApply(sql);
        onClose();
      }
    } else {
      handleAiBuild();
    }
  };

  const handleAiBuild = async () => {
    if (!description.trim()) {
      setError("Describe the join you want to build");
      return;
    }
    setIsBuilding(true);
    setError("");
    try {
      const result = await buildJoin({ description: description.trim() });
      onApply(result.sql);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to build join");
    } finally {
      setIsBuilding(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      handleGenerate();
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div
        className="w-full max-w-2xl rounded-xl bg-[var(--bg-primary)] border border-[var(--border)] shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
          <div className="flex items-center gap-2">
            <GitBranch className="w-5 h-5 text-[var(--accent)]" />
            <h2 className="text-base font-semibold text-[var(--text-primary)]">Join Builder</h2>
            {crossDbMode && (
              <span className="px-2 py-0.5 rounded-md bg-[var(--accent)]/10 text-[var(--accent)] text-xs font-medium flex items-center gap-1">
                <Database className="w-3 h-3" />
                Cross-DB
              </span>
            )}
          </div>
          <button onClick={onClose} className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] transition-colors">
            <X className="w-4 h-4 text-[var(--text-muted)]" />
          </button>
        </div>

        {/* Mode Toggle */}
        <div className="flex border-b border-[var(--border)]">
          <button
            onClick={() => setMode("manual")}
            className={`flex-1 px-4 py-2 text-sm font-medium transition-colors ${
              mode === "manual"
                ? "text-[var(--accent)] border-b-2 border-[var(--accent)] bg-[var(--accent)]/5"
                : "text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)]"
            }`}
          >
            Manual
          </button>
          <button
            onClick={() => setMode("ai")}
            className={`flex-1 px-4 py-2 text-sm font-medium transition-colors ${
              mode === "ai"
                ? "text-[var(--accent)] border-b-2 border-[var(--accent)] bg-[var(--accent)]/5"
                : "text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)]"
            }`}
          >
            AI Builder
          </button>
        </div>

        {/* Body */}
        <div className="p-5 space-y-4">
          {mode === "manual" ? (
            <>
              {/* Join Type */}
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-muted)]">Join Type</label>
                <div className="grid grid-cols-2 gap-2">
                  {JOIN_TYPES.map((jt) => (
                    <button
                      key={jt.value}
                      onClick={() => setJoinType(jt.value)}
                      className={`px-3 py-2 rounded-lg text-xs font-medium border transition-colors ${
                        joinType === jt.value
                          ? "border-[var(--accent)] bg-[var(--accent)]/10 text-[var(--accent)]"
                          : "border-[var(--border)] bg-[var(--bg-secondary)] text-[var(--text-secondary)] hover:bg-[var(--bg-tertiary)]"
                      }`}
                    >
                      {jt.label}
                      <div className="text-[10px] text-[var(--text-muted)] font-normal">{jt.desc}</div>
                    </button>
                  ))}
                </div>
              </div>

              {/* Tables - Enhanced for cross-database */}
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-muted)]">
                  Tables {crossDbMode && "(database.table format supported)"}
                </label>
                <div className="flex items-center gap-2">
                  <select
                    value={leftTable}
                    onChange={(e) => setLeftTable(e.target.value)}
                    className="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] text-sm focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
                  >
                    <option value="">Left table...</option>
                    {availableTables.map((t) => (
                      <option key={`left-${t.fullName}`} value={t.fullName}>{t.fullName}</option>
                    ))}
                  </select>
                  <ArrowRight className="w-4 h-4 text-[var(--text-muted)] flex-shrink-0" />
                  <select
                    value={rightTable}
                    onChange={(e) => setRightTable(e.target.value)}
                    className="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] text-sm focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
                  >
                    <option value="">Right table...</option>
                    {availableTables.map((t) => (
                      <option key={`right-${t.fullName}`} value={t.fullName}>{t.fullName}</option>
                    ))}
                  </select>
                </div>
                {crossDbMode && (
                  <p className="text-xs text-[var(--text-muted)] mt-1">
                    Tip: You can join tables across different databases using fully qualified names
                  </p>
                )}
              </div>

              {/* Join Columns */}
              <div className="flex items-center gap-2">
                <input
                  ref={inputRef}
                  type="text"
                  value={leftColumn}
                  onChange={(e) => setLeftColumn(e.target.value)}
                  placeholder="Left column (e.g. id)"
                  className="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] text-sm focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
                />
                <span className="text-xs text-[var(--text-muted)]">=</span>
                <input
                  type="text"
                  value={rightColumn}
                  onChange={(e) => setRightColumn(e.target.value)}
                  placeholder="Right column (e.g. user_id)"
                  className="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] text-sm focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
                />
              </div>

              {/* Columns to select */}
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-muted)]">Select Columns</label>
                <input
                  type="text"
                  value={columns}
                  onChange={(e) => setColumns(e.target.value)}
                  placeholder="* or t1.name, t2.email"
                  className="w-full px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] text-sm focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
                />
              </div>

              {/* WHERE */}
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-muted)]">WHERE (optional)</label>
                <input
                  type="text"
                  value={where}
                  onChange={(e) => setWhere(e.target.value)}
                  placeholder="t1.status = 'active'"
                  className="w-full px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] text-sm focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
                />
              </div>
            </>
          ) : (
            <>
              {/* AI Mode */}
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-muted)]">
                  Describe the join you want to build
                </label>
                <input
                  ref={inputRef}
                  type="text"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && handleAiBuild()}
                  placeholder={crossDbMode 
                    ? "e.g., join db1.users with db2.orders, include only active users"
                    : "e.g., join users with their orders, include only active users"
                  }
                  className="w-full px-3 py-2 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border)] text-[var(--text-primary)] placeholder-[var(--text-muted)] text-sm focus:outline-none focus:ring-2 focus:ring-[var(--accent)]"
                />
              </div>
              {availableTables.length > 0 && (
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-muted)]">
                    Available tables {crossDbMode && "(across databases)"}
                  </label>
                  <div className="flex flex-wrap gap-1 max-h-24 overflow-y-auto">
                    {availableTables.map((t) => (
                      <span
                        key={t.fullName}
                        className={`px-2 py-0.5 rounded-md border text-xs ${
                          t.fullName.includes('.')
                            ? "bg-[var(--accent)]/5 border-[var(--accent)]/30 text-[var(--accent)]"
                            : "bg-[var(--bg-secondary)] border-[var(--border)] text-[var(--text-secondary)]"
                        }`}
                      >
                        {t.fullName}
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </>
          )}

          {/* Error */}
          {error && (
            <div className="px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20 text-red-500 text-xs">
              {error}
            </div>
          )}

          {/* Actions */}
          <div className="flex items-center justify-end gap-2 pt-3 border-t border-[var(--border)]">
            <button
              onClick={onClose}
              className="px-4 py-2 rounded-lg bg-[var(--bg-tertiary)] text-[var(--text-secondary)] text-sm font-medium hover:bg-[var(--border)] transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleGenerate}
              disabled={isBuilding || (mode === "manual" ? (!leftTable || !rightTable) : !description.trim())}
              className="px-4 py-2 rounded-lg bg-[var(--accent)] text-white text-sm font-medium hover:bg-[var(--accent-hover)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
            >
              {isBuilding ? (
                <>
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  Building...
                </>
              ) : (
                <>
                  <Send className="w-3.5 h-3.5" />
                  Build & Apply
                </>
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

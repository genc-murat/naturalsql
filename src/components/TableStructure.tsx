import { useState, useEffect } from "react";
import {
  X,
  Loader2,
  Table2,
  Key,
  Link,
  Shield,
  FileText,
  HardDrive,
  Hash,
} from "lucide-react";
import { getTableStructure } from "../api";
import type { TableStructure as TableStructureType } from "../types";
import CodeMirror from "@uiw/react-codemirror";
import { sql, MySQL } from "@codemirror/lang-sql";
import { vscodeDark, vscodeLight } from "@uiw/codemirror-theme-vscode";

interface TableStructureProps {
  isOpen: boolean;
  onClose: () => void;
  database: string;
  table: string;
}

type StructureTab = "columns" | "indexes" | "constraints" | "ddl";

export function TableStructure({ isOpen, onClose, database, table }: TableStructureProps) {
  const [structure, setStructure] = useState<TableStructureType | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [activeTab, setActiveTab] = useState<StructureTab>("columns");
  const [isDark, setIsDark] = useState(() => {
    const saved = localStorage.getItem("naturalsql-theme");
    if (saved) return saved === "dark";
    return window.matchMedia("(prefers-color-scheme: dark)").matches;
  });

  useEffect(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains("dark"));
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!isOpen || !database || !table) return;
    setLoading(true);
    setError("");
    getTableStructure(database, table)
      .then(setStructure)
      .catch((err) => setError(err instanceof Error ? err.message : String(err)))
      .finally(() => setLoading(false));
  }, [isOpen, database, table]);

  if (!isOpen) return null;

  const tabs: { id: StructureTab; label: string; icon: React.ReactNode }[] = [
    { id: "columns", label: "Columns", icon: <Table2 className="w-3.5 h-3.5" /> },
    { id: "indexes", label: "Indexes", icon: <Hash className="w-3.5 h-3.5" /> },
    { id: "constraints", label: "Constraints", icon: <Shield className="w-3.5 h-3.5" /> },
    { id: "ddl", label: "DDL", icon: <FileText className="w-3.5 h-3.5" /> },
  ];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="w-[720px] max-h-[85vh] bg-[var(--bg-primary)] border border-[var(--border)] rounded-xl shadow-2xl flex flex-col overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
          <div className="flex items-center gap-3">
            <Table2 className="w-5 h-5 text-[var(--accent)]" />
            <div>
              <h2 className="text-sm font-semibold text-[var(--text-primary)]">
                {database}.{table}
              </h2>
              {structure && (
                <div className="flex items-center gap-3 mt-0.5 text-xs text-[var(--text-muted)]">
                  <span>{structure.status.engine}</span>
                  <span>{structure.status.collation}</span>
                  <span>{structure.stats.row_count.toLocaleString()} rows</span>
                  <span>{structure.stats.data_size_mb} MB data</span>
                  <span>{structure.stats.index_size_mb} MB index</span>
                </div>
              )}
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] transition-colors"
          >
            <X className="w-4 h-4 text-[var(--text-muted)]" />
          </button>
        </div>

        {/* Loading */}
        {loading && (
          <div className="flex items-center justify-center py-16">
            <Loader2 className="w-6 h-6 animate-spin text-[var(--accent)]" />
            <span className="ml-3 text-sm text-[var(--text-muted)]">Loading structure...</span>
          </div>
        )}

        {/* Error */}
        {error && !loading && (
          <div className="p-6 text-center">
            <p className="text-sm text-[var(--error)]">{error}</p>
          </div>
        )}

        {/* Content */}
        {structure && !loading && (
          <>
            {/* Tabs */}
            <div className="flex items-center gap-1 px-4 pt-2 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
              {tabs.map((tab) => (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id)}
                  className={`flex items-center gap-1.5 px-3 py-2 text-sm font-medium transition-colors border-b-2 ${
                    activeTab === tab.id
                      ? "border-[var(--accent)] text-[var(--accent)]"
                      : "border-transparent text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
                  }`}
                >
                  {tab.icon}
                  {tab.label}
                </button>
              ))}
            </div>

            {/* Tab Content */}
            <div className="flex-1 overflow-auto p-4">
              {activeTab === "columns" && (
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-[var(--border)]">
                      <th className="text-left py-2 px-3 text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Column</th>
                      <th className="text-left py-2 px-3 text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Type</th>
                      <th className="text-left py-2 px-3 text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Nullable</th>
                      <th className="text-left py-2 px-3 text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Key</th>
                    </tr>
                  </thead>
                  <tbody>
                    {structure.ddl.split("\n")
                      .filter((line) => line.trim().startsWith("`"))
                      .map((line, i) => {
                        const nameMatch = line.match(/`(\w+)`/);
                        const typeMatch = line.match(/`\w+`\s+(\S+)/);
                        const isNullable = !line.includes("NOT NULL");
                        const isPk = line.includes("PRIMARY KEY") || structure.constraints.some(
                          (c) => c.constraint_type === "PRIMARY KEY" && c.column === nameMatch?.[1]
                        );
                        return (
                          <tr key={i} className="border-b border-[var(--border)]/50 hover:bg-[var(--bg-tertiary)]/50">
                            <td className="py-2 px-3 font-mono text-[var(--text-primary)]">
                              <div className="flex items-center gap-2">
                                {nameMatch?.[1]}
                                {isPk && <Key className="w-3 h-3 text-yellow-500" />}
                              </div>
                            </td>
                            <td className="py-2 px-3 font-mono text-[var(--text-secondary)]">{typeMatch?.[1]}</td>
                            <td className="py-2 px-3">
                              <span className={`px-1.5 py-0.5 rounded text-xs ${isNullable ? "text-[var(--text-muted)]" : "text-[var(--error)]"}`}>
                                {isNullable ? "YES" : "NO"}
                              </span>
                            </td>
                            <td className="py-2 px-3">
                              {isPk && <span className="px-1.5 py-0.5 rounded text-xs bg-yellow-500/20 text-yellow-500">PRI</span>}
                            </td>
                          </tr>
                        );
                      })}
                  </tbody>
                </table>
              )}

              {activeTab === "indexes" && (
                structure.indexes.length === 0 ? (
                  <p className="text-sm text-[var(--text-muted)] text-center py-8">No indexes found</p>
                ) : (
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="border-b border-[var(--border)]">
                        <th className="text-left py-2 px-3 text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Name</th>
                        <th className="text-left py-2 px-3 text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Column</th>
                        <th className="text-left py-2 px-3 text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Type</th>
                        <th className="text-left py-2 px-3 text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Unique</th>
                      </tr>
                    </thead>
                    <tbody>
                      {structure.indexes.map((idx, i) => (
                        <tr key={i} className="border-b border-[var(--border)]/50 hover:bg-[var(--bg-tertiary)]/50">
                          <td className="py-2 px-3 font-mono text-[var(--text-primary)]">{idx.name}</td>
                          <td className="py-2 px-3 font-mono text-[var(--text-secondary)]">{idx.column}</td>
                          <td className="py-2 px-3 text-[var(--text-secondary)]">{idx.index_type}</td>
                          <td className="py-2 px-3">
                            {idx.non_unique ? (
                              <span className="text-xs text-[var(--text-muted)]">No</span>
                            ) : (
                              <span className="px-1.5 py-0.5 rounded text-xs bg-green-500/20 text-green-500">Yes</span>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )
              )}

              {activeTab === "constraints" && (
                structure.constraints.length === 0 && structure.foreign_keys.length === 0 ? (
                  <p className="text-sm text-[var(--text-muted)] text-center py-8">No constraints found</p>
                ) : (
                  <div className="space-y-4">
                    {structure.constraints.length > 0 && (
                      <div>
                        <h3 className="text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider mb-2">Table Constraints</h3>
                        <table className="w-full text-sm">
                          <thead>
                            <tr className="border-b border-[var(--border)]">
                              <th className="text-left py-2 px-3 text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Name</th>
                              <th className="text-left py-2 px-3 text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Column</th>
                              <th className="text-left py-2 px-3 text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Type</th>
                            </tr>
                          </thead>
                          <tbody>
                            {structure.constraints.map((c, i) => (
                              <tr key={i} className="border-b border-[var(--border)]/50 hover:bg-[var(--bg-tertiary)]/50">
                                <td className="py-2 px-3 font-mono text-[var(--text-primary)]">{c.name}</td>
                                <td className="py-2 px-3 font-mono text-[var(--text-secondary)]">{c.column}</td>
                                <td className="py-2 px-3">
                                  <span className={`px-1.5 py-0.5 rounded text-xs ${
                                    c.constraint_type === "PRIMARY KEY" ? "bg-yellow-500/20 text-yellow-500" :
                                    c.constraint_type === "UNIQUE" ? "bg-blue-500/20 text-blue-500" :
                                    c.constraint_type === "FOREIGN KEY" ? "bg-purple-500/20 text-purple-500" :
                                    "bg-[var(--bg-tertiary)] text-[var(--text-muted)]"
                                  }`}>
                                    {c.constraint_type}
                                  </span>
                                </td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    )}
                    {structure.foreign_keys.length > 0 && (
                      <div>
                        <h3 className="text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider mb-2">Foreign Keys</h3>
                        <div className="space-y-2">
                          {structure.foreign_keys.map((fk, i) => (
                            <div key={i} className="flex items-center gap-2 px-3 py-2 rounded-md bg-[var(--bg-tertiary)] border border-[var(--border)]">
                              <Link className="w-4 h-4 text-purple-400" />
                              <span className="font-mono text-sm text-[var(--text-primary)]">{fk.from_column}</span>
                              <span className="text-[var(--text-muted)]">&rarr;</span>
                              <span className="font-mono text-sm text-[var(--accent)]">{fk.to_database}.{fk.to_table}.{fk.to_column}</span>
                              {fk.constraint_name && (
                                <span className="text-xs text-[var(--text-muted)] ml-auto">{fk.constraint_name}</span>
                              )}
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                )
              )}

              {activeTab === "ddl" && (
                <div className="rounded-md border border-[var(--border)] overflow-hidden">
                  <CodeMirror
                    value={structure.ddl}
                    extensions={[sql({ dialect: MySQL })]}
                    theme={isDark ? vscodeDark : vscodeLight}
                    height="400px"
                    readOnly={true}
                    className="text-sm [&_.cm-editor]:!h-full [&_.cm-scroller]:!overflow-auto"
                    basicSetup={{
                      lineNumbers: true,
                      foldGutter: true,
                      highlightActiveLine: false,
                      syntaxHighlighting: true,
                    }}
                  />
                </div>
              )}
            </div>
          </>
        )}

        {/* Footer */}
        {structure && !loading && (
          <div className="flex items-center justify-between px-5 py-2.5 border-t border-[var(--border)] bg-[var(--bg-secondary)] text-xs text-[var(--text-muted)]">
            <div className="flex items-center gap-3">
              <span className="flex items-center gap-1"><HardDrive className="w-3 h-3" /> {structure.status.engine}</span>
              {structure.status.create_time && <span>Created: {structure.status.create_time}</span>}
            </div>
            {structure.status.auto_increment && (
              <span>Auto Increment: {structure.status.auto_increment.toLocaleString()}</span>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

import { useState, useEffect } from "react";
import { X, AlertTriangle, Loader2, Database, Play, Undo2, Eye } from "lucide-react";
import { executeSql } from "../api";
import type { DataEditResponse, QueryResult } from "../types";

interface DataEditPreviewProps {
  isOpen: boolean;
  onClose: () => void;
  edit: DataEditResponse | null;
  loading: boolean;
  onApply: () => void;
}

export function DataEditPreview({ isOpen, onClose, edit, loading, onApply }: DataEditPreviewProps) {
  const [executing, setExecuting] = useState(false);
  const [preview, setPreview] = useState<QueryResult | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    if (isOpen && edit?.preview_sql) {
      setPreviewLoading(true);
      executeSql({ sql: edit.preview_sql })
        .then(setPreview)
        .catch(() => setPreview(null))
        .finally(() => setPreviewLoading(false));
    }
  }, [isOpen, edit?.preview_sql]);

  if (!isOpen) return null;

  const handleExecute = async () => {
    if (!edit) return;
    setExecuting(true);
    setError("");
    setResult(null);
    try {
      const res = await executeSql({ sql: edit.sql });
      setResult(`Success. ${res.affected_rows ?? 0} row(s) affected.`);
      onApply();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setExecuting(false);
    }
  };

  const handleCopyUndo = () => {
    if (edit?.undo_sql) {
      navigator.clipboard.writeText(edit.undo_sql);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="w-[640px] max-h-[85vh] bg-[var(--bg-primary)] border border-[var(--border)] rounded-xl shadow-2xl flex flex-col overflow-hidden">
        <div className="flex items-center justify-between px-5 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
          <div className="flex items-center gap-2">
            <Database className="w-5 h-5 text-[var(--accent)]" />
            <h2 className="text-sm font-semibold text-[var(--text-primary)]">Data Edit Preview</h2>
          </div>
          <button onClick={onClose} className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)]">
            <X className="w-4 h-4 text-[var(--text-muted)]" />
          </button>
        </div>

        {loading && (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="w-5 h-5 animate-spin text-[var(--accent)]" />
            <span className="ml-2 text-sm text-[var(--text-muted)]">Generating edit SQL...</span>
          </div>
        )}

        {edit && !loading && (
          <div className="flex-1 overflow-auto p-5 space-y-4">
            {/* Warning */}
            <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-orange-500/10 border border-orange-500/30">
              <AlertTriangle className="w-4 h-4 text-orange-400" />
              <span className="text-sm text-orange-400">This will modify data in your database</span>
            </div>

            {/* Explanation */}
            {edit.explanation && (
              <p className="text-sm text-[var(--text-secondary)]">{edit.explanation}</p>
            )}

            {/* SQL */}
            <div>
              <label className="text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider mb-1.5 block">SQL Statement</label>
              <div className="p-3 rounded-md bg-[var(--bg-tertiary)] border border-[var(--border)] font-mono text-sm text-[var(--text-primary)] whitespace-pre-wrap">
                {edit.sql}
              </div>
            </div>

            {/* Preview */}
            {edit.preview_sql && (
              <div>
                <div className="flex items-center gap-1.5 mb-1.5">
                  <Eye className="w-3.5 h-3.5 text-[var(--accent)]" />
                  <label className="text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Affected Data Preview</label>
                </div>
                {previewLoading ? (
                  <div className="py-4 text-center">
                    <Loader2 className="w-4 h-4 animate-spin mx-auto text-[var(--accent)]" />
                  </div>
                ) : preview && preview.columns.length > 0 ? (
                  <div className="overflow-auto max-h-32 rounded-md border border-[var(--border)]">
                    <table className="w-full text-xs">
                      <thead className="bg-[var(--bg-tertiary)]">
                        <tr>
                          {preview.columns.map((c) => (
                            <th key={c} className="px-2 py-1 text-left font-mono text-[var(--text-muted)]">{c}</th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {preview.rows.slice(0, 5).map((row, i) => (
                          <tr key={i} className="border-t border-[var(--border)]/50">
                            {(row as unknown[]).map((val, j) => (
                              <td key={j} className="px-2 py-1 font-mono text-[var(--text-secondary)] max-w-[150px] truncate">{String(val ?? "NULL")}</td>
                            ))}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                    {preview.row_count > 5 && (
                      <div className="px-2 py-1 text-xs text-[var(--text-muted)] border-t border-[var(--border)]/50">
                        ... and {preview.row_count - 5} more rows
                      </div>
                    )}
                  </div>
                ) : (
                  <p className="text-xs text-[var(--text-muted)]">No preview data available</p>
                )}
              </div>
            )}

            {/* Undo SQL */}
            {edit.undo_sql && (
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <div className="flex items-center gap-1.5">
                    <Undo2 className="w-3.5 h-3.5 text-blue-400" />
                    <label className="text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">Undo SQL (save this)</label>
                  </div>
                  <button onClick={handleCopyUndo} className="text-xs text-[var(--accent)] hover:underline">Copy</button>
                </div>
                <div className="p-2 rounded-md bg-[var(--bg-tertiary)] border border-[var(--border)] font-mono text-xs text-[var(--text-muted)] whitespace-pre-wrap">
                  {edit.undo_sql}
                </div>
              </div>
            )}

            {result && (
              <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-green-500/10 border border-green-500/30">
                <span className="text-sm text-green-400">{result}</span>
              </div>
            )}
            {error && (
              <div className="px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/30">
                <p className="text-sm text-red-400">{error}</p>
              </div>
            )}
          </div>
        )}

        {edit && !loading && !result && (
          <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-[var(--border)] bg-[var(--bg-secondary)]">
            <button onClick={onClose} className="px-4 py-1.5 rounded-md text-sm text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)]">
              Cancel
            </button>
            <button
              onClick={handleExecute}
              disabled={executing}
              className="px-4 py-1.5 rounded-md bg-orange-500 text-white text-sm font-medium hover:bg-orange-600 disabled:opacity-40 flex items-center gap-1.5"
            >
              {executing ? <Loader2 className="w-4 h-4 animate-spin" /> : <Play className="w-4 h-4" />}
              Execute Edit
            </button>
          </div>
        )}

        {result && (
          <div className="flex items-center justify-end px-5 py-3 border-t border-[var(--border)] bg-[var(--bg-secondary)]">
            <button onClick={onClose} className="px-4 py-1.5 rounded-md text-sm text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)]">Close</button>
          </div>
        )}
      </div>
    </div>
  );
}

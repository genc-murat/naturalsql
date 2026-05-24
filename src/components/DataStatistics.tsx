import { useMemo } from "react";
import { Hash, TrendingUp, BarChart3 } from "lucide-react";

interface DataStatisticsProps {
  columns: string[];
  rows: unknown[][];
}

interface ColumnStat {
  name: string;
  type: "numeric" | "text" | "date" | "boolean" | "null";
  count: number;
  nullCount: number;
  uniqueCount: number;
  min?: string;
  max?: string;
  avg?: string;
  sum?: string;
  median?: string;
  topValues: { value: string; count: number }[];
}

function toNumber(v: unknown): number | null {
  if (typeof v === "number") return v;
  if (typeof v === "string") {
    const n = Number(v);
    return isNaN(n) ? null : n;
  }
  return null;
}

function computeStats(columns: string[], rows: unknown[][]): ColumnStat[] {
  return columns.map((name, idx) => {
    const values = rows.map((r) => r[idx]);
    const nonNull = values.filter((v) => v !== null && v !== undefined);
    const nullCount = values.length - nonNull.length;

    const numericValues = nonNull.map(toNumber).filter((v): v is number => v !== null);
    const stringValues = nonNull.map(String);

    const isNumeric = numericValues.length > nonNull.length * 0.7;

    const freq = new Map<string, number>();
    for (const s of stringValues) {
      freq.set(s, (freq.get(s) || 0) + 1);
    }
    const topValues = Array.from(freq.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, 5)
      .map(([value, count]) => ({ value, count }));

    let type: ColumnStat["type"] = "text";
    if (nonNull.length === 0) {
      type = "null";
    } else if (isNumeric) {
      type = "numeric";
    } else if (nonNull.every((v) => v === 0 || v === 1 || v === true || v === false)) {
      type = "boolean";
    }

    const stat: ColumnStat = {
      name,
      type,
      count: nonNull.length,
      nullCount,
      uniqueCount: freq.size,
      topValues,
    };

    if (isNumeric && numericValues.length > 0) {
      const sorted = [...numericValues].sort((a, b) => a - b);
      stat.min = sorted[0].toString();
      stat.max = sorted[sorted.length - 1].toString();
      const sum = numericValues.reduce((a, b) => a + b, 0);
      stat.avg = (sum / numericValues.length).toFixed(2);
      stat.sum = sum % 1 === 0 ? sum.toString() : sum.toFixed(2);
      const mid = Math.floor(sorted.length / 2);
      stat.median = sorted.length % 2 === 0
        ? ((sorted[mid - 1] + sorted[mid]) / 2).toString()
        : sorted[mid].toString();
    }

    return stat;
  });
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

const TYPE_BADGES: Record<string, { bg: string; text: string }> = {
  numeric: { bg: "bg-blue-500/10", text: "text-blue-400" },
  text: { bg: "bg-green-500/10", text: "text-green-400" },
  date: { bg: "bg-purple-500/10", text: "text-purple-400" },
  boolean: { bg: "bg-amber-500/10", text: "text-amber-400" },
  null: { bg: "bg-gray-500/10", text: "text-gray-400" },
};

export function DataStatistics({ columns, rows }: DataStatisticsProps) {
  const stats = useMemo(() => computeStats(columns, rows), [columns, rows]);

  if (rows.length === 0) {
    return (
      <div className="flex items-center justify-center h-64 text-[var(--text-muted)]">
        <div className="text-center">
          <BarChart3 className="w-10 h-10 mx-auto mb-3 opacity-30" />
          <p className="text-sm">No data to analyze</p>
        </div>
      </div>
    );
  }

  const numericCols = stats.filter((s) => s.type === "numeric");
  const totalRows = rows.length;

  return (
    <div className="space-y-6">
      {numericCols.length > 0 && (
        <div className="rounded-xl border border-[var(--border)] bg-[var(--bg-secondary)] overflow-hidden">
          <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--border)]">
            <TrendingUp className="w-4 h-4 text-blue-400" />
            <span className="text-sm font-medium text-[var(--text-primary)]">Numeric Summary</span>
            <span className="ml-auto text-xs text-[var(--text-muted)]">{numericCols.length} columns</span>
          </div>
          <div className="overflow-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-[var(--border)] bg-[var(--bg-tertiary)]">
                  <th className="px-4 py-2 text-left font-medium text-[var(--text-muted)]">Column</th>
                  <th className="px-4 py-2 text-right font-medium text-[var(--text-muted)]">Min</th>
                  <th className="px-4 py-2 text-right font-medium text-[var(--text-muted)]">Max</th>
                  <th className="px-4 py-2 text-right font-medium text-[var(--text-muted)]">Avg</th>
                  <th className="px-4 py-2 text-right font-medium text-[var(--text-muted)]">Median</th>
                  <th className="px-4 py-2 text-right font-medium text-[var(--text-muted)]">Sum</th>
                  <th className="px-4 py-2 text-right font-medium text-[var(--text-muted)]">Unique</th>
                  <th className="px-4 py-2 text-right font-medium text-[var(--text-muted)]">NULLs</th>
                </tr>
              </thead>
              <tbody>
                {numericCols.map((s) => (
                  <tr key={s.name} className="border-b border-[var(--border)] hover:bg-[var(--bg-tertiary)]/50">
                    <td className="px-4 py-2 font-mono text-[var(--text-primary)]">{s.name}</td>
                    <td className="px-4 py-2 text-right text-[var(--text-secondary)] font-mono">{s.min ?? "-"}</td>
                    <td className="px-4 py-2 text-right text-[var(--text-secondary)] font-mono">{s.max ?? "-"}</td>
                    <td className="px-4 py-2 text-right text-blue-400 font-mono">{s.avg ?? "-"}</td>
                    <td className="px-4 py-2 text-right text-purple-400 font-mono">{s.median ?? "-"}</td>
                    <td className="px-4 py-2 text-right text-green-400 font-mono">{s.sum ?? "-"}</td>
                    <td className="px-4 py-2 text-right text-[var(--text-muted)]">{formatNumber(s.uniqueCount)}</td>
                    <td className="px-4 py-2 text-right">
                      {s.nullCount > 0 ? (
                        <span className="text-amber-400">{s.nullCount}</span>
                      ) : (
                        <span className="text-[var(--text-muted)]">0</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {stats.map((s) => {
          const badge = TYPE_BADGES[s.type] || TYPE_BADGES.text;
          return (
            <div key={s.name} className="rounded-xl border border-[var(--border)] bg-[var(--bg-secondary)] p-4">
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-2">
                  <Hash className="w-4 h-4 text-[var(--text-muted)]" />
                  <span className="font-mono text-sm font-medium text-[var(--text-primary)]">{s.name}</span>
                </div>
                <span className={`px-2 py-0.5 rounded-md text-[10px] font-mono uppercase ${badge.bg} ${badge.text}`}>
                  {s.type}
                </span>
              </div>

              <div className="grid grid-cols-3 gap-2 mb-3">
                <div className="px-2 py-1.5 rounded-md bg-[var(--bg-tertiary)] text-center">
                  <div className="text-[10px] text-[var(--text-muted)] mb-0.5">Count</div>
                  <div className="text-sm font-mono text-[var(--text-primary)]">{formatNumber(s.count)}</div>
                </div>
                <div className="px-2 py-1.5 rounded-md bg-[var(--bg-tertiary)] text-center">
                  <div className="text-[10px] text-[var(--text-muted)] mb-0.5">Unique</div>
                  <div className="text-sm font-mono text-[var(--text-primary)]">{formatNumber(s.uniqueCount)}</div>
                </div>
                <div className="px-2 py-1.5 rounded-md bg-[var(--bg-tertiary)] text-center">
                  <div className="text-[10px] text-[var(--text-muted)] mb-0.5">Null</div>
                  <div className={`text-sm font-mono ${s.nullCount > 0 ? "text-amber-400" : "text-[var(--text-primary)]"}`}>
                    {s.nullCount > 0 ? `${((s.nullCount / totalRows) * 100).toFixed(1)}%` : "0"}
                  </div>
                </div>
              </div>

              {s.topValues.length > 0 && (
                <div>
                  <div className="text-[10px] text-[var(--text-muted)] mb-1.5 font-medium">Top Values</div>
                  <div className="space-y-1">
                    {s.topValues.map((tv) => {
                      const pct = (tv.count / totalRows) * 100;
                      return (
                        <div key={tv.value} className="flex items-center gap-2">
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center justify-between text-xs">
                              <span className="text-[var(--text-secondary)] truncate font-mono">{tv.value}</span>
                              <span className="text-[var(--text-muted)] ml-2 shrink-0">{tv.count}x</span>
                            </div>
                            <div className="h-1 rounded-full bg-[var(--bg-tertiary)] mt-0.5">
                              <div
                                className="h-full rounded-full bg-[var(--accent)]"
                                style={{ width: `${Math.max(pct, 2)}%` }}
                              />
                            </div>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}

              {s.type === "numeric" && (
                <div className="mt-3 grid grid-cols-4 gap-1">
                  {[
                    { label: "Min", val: s.min },
                    { label: "Max", val: s.max },
                    { label: "Avg", val: s.avg },
                    { label: "Med", val: s.median },
                  ].map(({ label, val }) => (
                    <div key={label} className="px-2 py-1 rounded bg-[var(--bg-tertiary)] text-center">
                      <div className="text-[9px] text-[var(--text-muted)]">{label}</div>
                      <div className="text-xs font-mono text-[var(--text-secondary)]">{val ?? "-"}</div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

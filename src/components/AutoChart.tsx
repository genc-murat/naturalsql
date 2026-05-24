import { useMemo } from "react";
import {
  BarChart,
  Bar,
  LineChart,
  Line,
  PieChart,
  Pie,
  Cell,
  AreaChart,
  Area,
  ScatterChart,
  Scatter,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
  ResponsiveContainer,
  RadarChart,
  PolarGrid,
  PolarAngleAxis,
  PolarRadiusAxis,
  Radar,
} from "recharts";
import { BarChart3, TrendingUp, PieChart as PieIcon, Activity } from "lucide-react";

interface AutoChartProps {
  columns: string[];
  rows: unknown[][];
}

interface ColumnProfile {
  name: string;
  type: "numeric" | "categorical" | "date" | "unknown";
  uniqueValues: number;
  sample: string[];
}

const COLORS = [
  "#38bdf8", "#818cf8", "#a78bfa", "#c084fc", "#e879f9",
  "#f472b6", "#fb7185", "#f87171", "#fb923c", "#fbbf24",
  "#a3e635", "#4ade80", "#2dd4bf", "#22d3ee", "#60a5fa",
];

const CHART_COLORS = {
  grid: "var(--border)",
  muted: "var(--text-muted)",
};

function isNumeric(value: unknown): boolean {
  if (typeof value === "number") return true;
  if (typeof value === "string") {
    if (value.trim() === "") return false;
    return !isNaN(Number(value));
  }
  return false;
}

function isDateLike(value: unknown): boolean {
  if (typeof value !== "string") return false;
  const d = new Date(value);
  return !isNaN(d.getTime()) && value.length > 4;
}

function toNumber(v: unknown): number {
  if (typeof v === "number") return v;
  if (typeof v === "string") return Number(v) || 0;
  return 0;
}

function profileColumns(columns: string[], rows: unknown[][]): ColumnProfile[] {
  return columns.map((name, idx) => {
    const values = rows.map((r) => r[idx]).filter((v) => v !== null && v !== undefined);
    const sample = values.slice(0, 5).map(String);

    const numericCount = values.filter(isNumeric).length;
    const dateCount = values.filter(isDateLike).length;
    const uniqueSet = new Set(values.map(String));

    let type: ColumnProfile["type"] = "unknown";
    if (values.length > 0) {
      if (dateCount > values.length * 0.7 && numericCount < values.length * 0.5) {
        type = "date";
      } else if (numericCount > values.length * 0.7) {
        type = "numeric";
      } else {
        type = "categorical";
      }
    }

    return { name, type, uniqueValues: uniqueSet.size, sample };
  });
}

interface ChartSuggestion {
  type: "bar" | "line" | "area" | "pie" | "scatter" | "radar";
  label: string;
  icon: React.ReactNode;
  xCol: string;
  yCols: string[];
  score: number;
}

function suggestCharts(profiles: ColumnProfile[], rowCount: number): ChartSuggestion[] {
  const suggestions: ChartSuggestion[] = [];
  const categoricals = profiles.filter((p) => p.type === "categorical");
  const numerics = profiles.filter((p) => p.type === "numeric");
  const dates = profiles.filter((p) => p.type === "date");

  if (categoricals.length >= 1 && numerics.length >= 1) {
    const cat = categoricals[0];
    suggestions.push({
      type: "bar",
      label: `${cat.name} vs ${numerics.map((n) => n.name).join(", ")}`,
      icon: <BarChart3 className="w-4 h-4" />,
      xCol: cat.name,
      yCols: numerics.slice(0, 3).map((n) => n.name),
      score: 10,
    });

    if (cat.uniqueValues <= 8 && numerics.length === 1) {
      suggestions.push({
        type: "pie",
        label: `${numerics[0].name} distribution by ${cat.name}`,
        icon: <PieIcon className="w-4 h-4" />,
        xCol: cat.name,
        yCols: [numerics[0].name],
        score: 8,
      });
    }

    if (cat.uniqueValues <= 12 && numerics.length >= 2) {
      suggestions.push({
        type: "radar",
        label: `${numerics.slice(0, 2).map((n) => n.name).join(" vs ")} by ${cat.name}`,
        icon: <Activity className="w-4 h-4" />,
        xCol: cat.name,
        yCols: numerics.slice(0, 4).map((n) => n.name),
        score: 6,
      });
    }
  }

  if (dates.length >= 1 && numerics.length >= 1) {
    suggestions.push({
      type: "line",
      label: `${numerics[0].name} over ${dates[0].name}`,
      icon: <TrendingUp className="w-4 h-4" />,
      xCol: dates[0].name,
      yCols: numerics.slice(0, 3).map((n) => n.name),
      score: 10,
    });
    suggestions.push({
      type: "area",
      label: `${numerics[0].name} trend over ${dates[0].name}`,
      icon: <TrendingUp className="w-4 h-4" />,
      xCol: dates[0].name,
      yCols: numerics.slice(0, 2).map((n) => n.name),
      score: 8,
    });
  }

  if (numerics.length >= 2) {
    suggestions.push({
      type: "scatter",
      label: `${numerics[0].name} vs ${numerics[1].name}`,
      icon: <Activity className="w-4 h-4" />,
      xCol: numerics[0].name,
      yCols: [numerics[1].name],
      score: 7,
    });
  }

  if (categoricals.length === 0 && numerics.length >= 1 && rowCount <= 100) {
    suggestions.push({
      type: "bar",
      label: `Row index vs ${numerics[0].name}`,
      icon: <BarChart3 className="w-4 h-4" />,
      xCol: "__index__",
      yCols: numerics.slice(0, 2).map((n) => n.name),
      score: 4,
    });
  }

  return suggestions.sort((a, b) => b.score - a.score);
}

export function AutoChart({ columns, rows }: AutoChartProps) {
  const profiles = useMemo(() => profileColumns(columns, rows), [columns, rows]);
  const charts = useMemo(() => suggestCharts(profiles, rows.length), [profiles, rows]);

  const chartData = useMemo(() => {
    return rows.map((row, idx) => {
      const obj: Record<string, unknown> = { __index__: idx + 1 };
      columns.forEach((col, i) => {
        obj[col] = row[i];
      });
      return obj;
    });
  }, [columns, rows]);

  if (charts.length === 0) {
    return (
      <div className="flex items-center justify-center h-64 text-[var(--text-muted)]">
        <div className="text-center">
          <BarChart3 className="w-10 h-10 mx-auto mb-3 opacity-30" />
          <p className="text-sm">No suitable chart found for this data</p>
          <p className="text-xs mt-1">Charts require at least one numeric column</p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {charts.map((chart, chartIdx) => (
        <div key={chartIdx} className="rounded-xl border border-[var(--border)] bg-[var(--bg-secondary)] overflow-hidden">
          <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--border)]">
            <span className="text-[var(--accent)]">{chart.icon}</span>
            <span className="text-sm font-medium text-[var(--text-primary)]">{chart.label}</span>
            <span className="ml-auto px-2 py-0.5 rounded-md bg-[var(--bg-tertiary)] text-[10px] font-mono text-[var(--text-muted)] uppercase">
              {chart.type}
            </span>
          </div>
          <div className="p-4">
            <ResponsiveContainer width="100%" height={300}>
              {renderChart(chart, chartData)}
            </ResponsiveContainer>
          </div>
        </div>
      ))}
    </div>
  );
}

function renderChart(chart: ChartSuggestion, data: Record<string, unknown>[]) {
  const aggData = chart.type === "pie" || chart.type === "bar" || chart.type === "radar"
    ? aggregateData(data, chart)
    : data;

  const tooltipStyle = {
    contentStyle: {
      backgroundColor: "var(--bg-secondary)",
      border: "1px solid var(--border)",
      borderRadius: "8px",
      fontSize: "12px",
      color: "var(--text-primary)",
    },
    itemStyle: { color: "var(--text-secondary)" },
    labelStyle: { color: "var(--text-primary)", fontWeight: 600 },
  };

  const axisStyle = {
    tick: { fill: CHART_COLORS.muted, fontSize: 11 },
    axisLine: { stroke: CHART_COLORS.grid },
    tickLine: { stroke: CHART_COLORS.grid },
  };

  switch (chart.type) {
    case "bar":
      return (
        <BarChart data={aggData} margin={{ top: 5, right: 20, bottom: 5, left: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke={CHART_COLORS.grid} />
          <XAxis dataKey={chart.xCol === "__index__" ? "__index__" : chart.xCol} {...axisStyle} />
          <YAxis {...axisStyle} />
          <Tooltip {...tooltipStyle} />
          <Legend />
          {chart.yCols.map((col, i) => (
            <Bar key={col} dataKey={col} fill={COLORS[i % COLORS.length]} radius={[4, 4, 0, 0]} />
          ))}
        </BarChart>
      );

    case "line":
      return (
        <LineChart data={aggData} margin={{ top: 5, right: 20, bottom: 5, left: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke={CHART_COLORS.grid} />
          <XAxis dataKey={chart.xCol} {...axisStyle} />
          <YAxis {...axisStyle} />
          <Tooltip {...tooltipStyle} />
          <Legend />
          {chart.yCols.map((col, i) => (
            <Line
              key={col}
              type="monotone"
              dataKey={col}
              stroke={COLORS[i % COLORS.length]}
              strokeWidth={2}
              dot={aggData.length < 50}
              activeDot={{ r: 5 }}
            />
          ))}
        </LineChart>
      );

    case "area":
      return (
        <AreaChart data={aggData} margin={{ top: 5, right: 20, bottom: 5, left: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke={CHART_COLORS.grid} />
          <XAxis dataKey={chart.xCol} {...axisStyle} />
          <YAxis {...axisStyle} />
          <Tooltip {...tooltipStyle} />
          <Legend />
          {chart.yCols.map((col, i) => (
            <Area
              key={col}
              type="monotone"
              dataKey={col}
              stroke={COLORS[i % COLORS.length]}
              fill={COLORS[i % COLORS.length]}
              fillOpacity={0.15}
              strokeWidth={2}
            />
          ))}
        </AreaChart>
      );

    case "pie":
      return (
        <PieChart>
          <Pie
            data={aggData}
            dataKey={chart.yCols[0]}
            nameKey={chart.xCol}
            cx="50%"
            cy="50%"
            outerRadius={120}
            innerRadius={60}
            paddingAngle={2}
            label={({ name, percent }: { name?: string; percent?: number }) =>
              `${name ?? ""} (${((percent ?? 0) * 100).toFixed(0)}%)`
            }
            labelLine={{ stroke: CHART_COLORS.muted }}
          >
            {aggData.map((_, i) => (
              <Cell key={i} fill={COLORS[i % COLORS.length]} />
            ))}
          </Pie>
          <Tooltip {...tooltipStyle} />
          <Legend />
        </PieChart>
      );

    case "scatter":
      return (
        <ScatterChart margin={{ top: 5, right: 20, bottom: 5, left: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke={CHART_COLORS.grid} />
          <XAxis dataKey={chart.xCol} name={chart.xCol} {...axisStyle} type="number" />
          <YAxis dataKey={chart.yCols[0]} name={chart.yCols[0]} {...axisStyle} />
          <Tooltip {...tooltipStyle} cursor={{ strokeDasharray: "3 3" }} />
          <Scatter data={aggData} fill={COLORS[0]} opacity={0.7} />
        </ScatterChart>
      );

    case "radar":
      return (
        <RadarChart data={aggData} cx="50%" cy="50%" outerRadius={120}>
          <PolarGrid stroke={CHART_COLORS.grid} />
          <PolarAngleAxis dataKey={chart.xCol} tick={{ fill: CHART_COLORS.muted, fontSize: 11 }} />
          <PolarRadiusAxis tick={{ fill: CHART_COLORS.muted, fontSize: 10 }} />
          {chart.yCols.map((col, i) => (
            <Radar
              key={col}
              name={col}
              dataKey={col}
              stroke={COLORS[i % COLORS.length]}
              fill={COLORS[i % COLORS.length]}
              fillOpacity={0.15}
            />
          ))}
          <Tooltip {...tooltipStyle} />
          <Legend />
        </RadarChart>
      );

    default:
      return null;
  }
}

function aggregateData(
  data: Record<string, unknown>[],
  chart: ChartSuggestion
): Record<string, unknown>[] {
  if (chart.type === "scatter") {
    return data.map((row) => ({
      [chart.xCol]: toNumber(row[chart.xCol]),
      [chart.yCols[0]]: toNumber(row[chart.yCols[0]]),
    }));
  }

  const xCol = chart.xCol === "__index__" ? "__index__" : chart.xCol;
  const groups = new Map<string, Record<string, number>>();

  for (const row of data) {
    const key = String(row[xCol] ?? "Unknown");
    if (!groups.has(key)) {
      const entry: Record<string, number> = {};
      for (const yc of chart.yCols) entry[yc] = 0;
      entry.__count = 0;
      groups.set(key, entry);
    }
    const g = groups.get(key)!;
    g.__count++;
    for (const yc of chart.yCols) {
      g[yc] += toNumber(row[yc]);
    }
  }

  return Array.from(groups.entries())
    .slice(0, 30)
    .map(([key, vals]) => {
      const entry: Record<string, unknown> = { [xCol]: key };
      for (const yc of chart.yCols) {
        entry[yc] = Math.round(vals[yc] * 100) / 100;
      }
      return entry;
    });
}

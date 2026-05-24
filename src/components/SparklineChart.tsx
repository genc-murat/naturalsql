import { LineChart, Line, ResponsiveContainer } from "recharts";

interface SparklineChartProps {
  values: number[];
  color?: string;
  height?: number;
}

export function SparklineChart({ values, color = "var(--accent)", height = 24 }: SparklineChartProps) {
  if (values.length < 2) return null;

  const data = values.map((v, i) => ({ x: i, v }));

  return (
    <ResponsiveContainer width="100%" height={height}>
      <LineChart data={data} margin={{ top: 2, right: 2, bottom: 2, left: 2 }}>
        <Line
          type="monotone"
          dataKey="v"
          stroke={color}
          strokeWidth={1.5}
          dot={false}
          isAnimationActive={false}
        />
      </LineChart>
    </ResponsiveContainer>
  );
}

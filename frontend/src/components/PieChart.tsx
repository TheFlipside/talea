/** Hand-rolled SVG pie chart summarizing the expense slices (no chart lib). */

export interface PieSlice {
  /** Stable, unique identity for React keys (e.g. the category id). */
  key: string;
  /** Positive magnitude of the slice (display-only number). */
  value: number;
  /** Fill color (see `lib/chart`). */
  color: string;
  /** Category emoji shown inside the slice. */
  icon: string;
}

interface PieChartProps {
  slices: PieSlice[];
  /** Accessible summary, e.g. "Expenses by category". */
  label: string;
}

const SIZE = 120;
const R = 54;
const C = SIZE / 2;
/** Radius at which a slice's icon/percent label sits (a touch inside the edge). */
const LABEL_R = R * 0.62;
/** Below this share a slice is too thin for an in-slice label (the bar shows it). */
const LABEL_MIN_PERCENT = 10;

/** A point on a circle of `radius` at `degrees` (0° = top, increasing clockwise). */
function pointAt(degrees: number, radius: number): { x: number; y: number } {
  const radians = ((degrees - 90) * Math.PI) / 180;
  return { x: C + radius * Math.cos(radians), y: C + radius * Math.sin(radians) };
}

/** The SVG arc path for a slice that sweeps `sweep` degrees from `startAngle`. */
function slicePath(startAngle: number, sweep: number): string {
  const start = pointAt(startAngle, R);
  const end = pointAt(startAngle + sweep, R);
  const largeArc = sweep > 180 ? 1 : 0;
  return `M ${C} ${C} L ${start.x} ${start.y} A ${R} ${R} 0 ${largeArc} 1 ${end.x} ${end.y} Z`;
}

/** Running start angle for each sweep (`[0, s0, s0+s1, …]`) in a single pass. */
function cumulativeStartAngles(sweeps: number[]): number[] {
  const starts: number[] = [];
  let total = 0;
  for (const sweep of sweeps) {
    starts.push(total);
    total += sweep;
  }
  return starts;
}

export function PieChart({ slices, label }: PieChartProps) {
  const positive = slices.filter((slice) => slice.value > 0);
  const total = positive.reduce((sum, slice) => sum + slice.value, 0);
  if (total <= 0) {
    return null;
  }
  const single = positive.length === 1;

  // Each slice's sweep and its cumulative start angle, then its full geometry,
  // so the render below is a declarative map (no render-phase mutation).
  const sweeps = positive.map((slice) => (slice.value / total) * 360);
  const startAngles = cumulativeStartAngles(sweeps);
  const arcs = positive.map((slice, index) => {
    const startAngle = startAngles[index];
    const sweep = sweeps[index];
    return {
      key: slice.key,
      path: slicePath(startAngle, sweep),
      color: slice.color,
      icon: slice.icon,
      percent: (slice.value / total) * 100,
      // A single full slice is centered; otherwise label at the slice centroid.
      label: single ? { x: C, y: C } : pointAt(startAngle + sweep / 2, LABEL_R),
    };
  });

  return (
    <svg className="pie-chart" viewBox={`0 0 ${SIZE} ${SIZE}`} role="img" aria-label={label}>
      {/* A single slice is a full circle — an arc path can't express 360°. */}
      {single ? (
        <circle cx={C} cy={C} r={R} fill={arcs[0].color} />
      ) : (
        arcs.map((arc) => <path key={arc.key} d={arc.path} fill={arc.color} />)
      )}
      {arcs.map((arc) =>
        arc.percent >= LABEL_MIN_PERCENT ? (
          <g key={`label-${arc.key}`} className="pie-chart__label">
            <text className="pie-chart__icon" x={arc.label.x} y={arc.label.y - 6} textAnchor="middle">
              {arc.icon}
            </text>
            <text className="pie-chart__pct" x={arc.label.x} y={arc.label.y + 7} textAnchor="middle">
              {Math.round(arc.percent)}%
            </text>
          </g>
        ) : null,
      )}
    </svg>
  );
}

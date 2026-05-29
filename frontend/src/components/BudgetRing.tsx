/** Hand-rolled SVG ring showing the spent fraction of the month's funds. */

interface BudgetRingProps {
  /** Fraction spent, 0..=1 (clamped by the caller). */
  spentFraction: number;
  /** Whether the month is overspent (tints the ring). */
  overspent: boolean;
  /** Accessible summary, e.g. "70% of this month's funds spent". */
  label: string;
}

export function BudgetRing({ spentFraction, overspent, label }: BudgetRingProps) {
  const fraction = Math.min(Math.max(spentFraction, 0), 1);
  const stroke = overspent ? 'var(--error)' : 'var(--accent)';

  return (
    <svg className="budget-ring" viewBox="0 0 120 120" role="img" aria-label={label}>
      <circle className="budget-ring__track" cx="60" cy="60" r="52" pathLength={1} />
      <circle
        className="budget-ring__value"
        cx="60"
        cy="60"
        r="52"
        pathLength={1}
        stroke={stroke}
        strokeDasharray={`${fraction} ${1 - fraction}`}
        transform="rotate(-90 60 60)"
      />
      <text className="budget-ring__pct" x="60" y="60" dominantBaseline="central" textAnchor="middle">
        {`${Math.round(fraction * 100)}%`}
      </text>
    </svg>
  );
}

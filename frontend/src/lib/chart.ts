/** Display-only chart helpers: a fixed slice palette shared by the stats pie
 *  chart and the bars beneath it, so a slice maps to its bar by color. */

import type { CategoryExpense } from '../api/types';

/**
 * Distinct slice colors, chosen to read on both the dark and light themes.
 * Assigned to categories in the order they appear (descending by amount).
 */
export const CHART_COLORS: readonly string[] = [
  '#40c9a2', // teal (the app accent)
  '#5b8def', // blue
  '#f4b740', // amber
  '#ef6f6c', // coral
  '#9b6dff', // violet
  '#f29bd4', // pink
  '#5ad1e0', // cyan
  '#a3c644', // lime
];

/** A muted, theme-neutral color reserved for the "Other" (uncategorized) slice. */
export const OTHER_COLOR = '#8a96a3';

/**
 * Assigns a stable color to each expense row: the `null` ("Other") bucket always
 * gets {@link OTHER_COLOR} wherever it appears; every real category takes the
 * next palette color in row order, cycling if there are more categories than
 * colors. Rows are expected sorted descending by amount (Other last), so the
 * order — and thus each color — is stable across renders for the same month.
 */
export function assignSliceColors(rows: CategoryExpense[]): string[] {
  let next = 0;
  return rows.map((row) =>
    row.category_id === null ? OTHER_COLOR : CHART_COLORS[next++ % CHART_COLORS.length],
  );
}

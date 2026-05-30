import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';

import type { Account, Category, CategoryId, CommandError } from '../api/types';
import { useCategories, useExpensesByCategory, useMonthSummary } from '../api/hooks';
import { ErrorBanner } from '../components/ErrorBanner';
import { MonthNav } from '../components/MonthNav';
import { Spinner } from '../components/Spinner';
import { categoryIconText } from '../lib/categories';
import { formatMoney, parseMoneyForDisplay } from '../lib/money';
import { useSwipe } from '../lib/swipe';
import { useSelectedMonth } from '../state/contexts';

/**
 * The statistics screen: a breakdown of the selected month's expenses by
 * category. Uncategorized expenses are shown as a single "Other" slice (the
 * backend buckets them under a `null` category id). Income is not shown.
 */
export function StatsScreen({ account }: { account: Account }) {
  const { t } = useTranslation();
  const { month, next, prev } = useSelectedMonth();
  const breakdown = useExpensesByCategory(account.id, month);
  const summary = useMonthSummary(account.id, month);
  const { data: categories } = useCategories();

  // Swipe paging mirrors the month screen (left → next, right → previous).
  const swipe = useSwipe({ onSwipeLeft: next, onSwipeRight: prev });

  const categoryById = useMemo(() => {
    const map = new Map<CategoryId, Category>();
    for (const category of categories ?? []) {
      map.set(category.id, category);
    }
    return map;
  }, [categories]);

  // Icon + label for a row; `null` (and any stale id) renders as "Other".
  function resolve(categoryId: CategoryId | null): { icon: string; label: string } {
    const category = categoryId === null ? undefined : categoryById.get(categoryId);
    return category
      ? { icon: categoryIconText(category.icon), label: category.label }
      : { icon: '🏷️', label: t('stats.other') };
  }

  return (
    <section className="screen stats-screen" {...swipe}>
      <div className="screen__header">
        <h2>{t('stats.title')}</h2>
      </div>
      <MonthNav />

      <StatsBody
        isPending={breakdown.isPending || summary.isPending}
        error={breakdown.error ?? summary.error}
        rows={breakdown.data ?? []}
        totalExpenses={summary.data?.expenses ?? '0'}
        currency={account.currency}
        resolve={resolve}
        emptyLabel={t('stats.empty')}
        totalLabel={t('stats.total')}
      />
    </section>
  );
}

interface StatsBodyProps {
  isPending: boolean;
  error: CommandError | null;
  rows: { category_id: CategoryId | null; total: string }[];
  totalExpenses: string;
  currency: string;
  resolve: (categoryId: CategoryId | null) => { icon: string; label: string };
  emptyLabel: string;
  totalLabel: string;
}

function StatsBody({
  isPending,
  error,
  rows,
  totalExpenses,
  currency,
  resolve,
  emptyLabel,
  totalLabel,
}: StatsBodyProps) {
  if (isPending) {
    return <Spinner />;
  }
  if (error) {
    return <ErrorBanner error={error} />;
  }
  if (rows.length === 0) {
    return <p className="muted">{emptyLabel}</p>;
  }

  // Display-only denominator for the bar/percent ratios (the audited chokepoint).
  // Summed from the rows themselves — not the separate `totalExpenses` query — so
  // the bars are always internally consistent (each row's share is its fraction
  // of the same data), with no transient mismatch while the two queries settle.
  // This is a visual ratio only; the authoritative total is the money string.
  const denominator = rows.reduce((sum, row) => sum + parseMoneyForDisplay(row.total), 0);

  return (
    <>
      <div className="stats__total">
        <span className="muted">{totalLabel}</span>
        <span className="amount amount--expense">{formatMoney(totalExpenses, currency)}</span>
      </div>
      <ul className="stats-list">
        {rows.map((row, index) => {
          const { icon, label } = resolve(row.category_id);
          const percent =
            denominator > 0 ? (parseMoneyForDisplay(row.total) / denominator) * 100 : 0;
          // category_id is unique per row (the backend merges by category, one
          // null "Other" bucket); index keeps the key unique even if that ever
          // changes.
          return (
            <li key={`${row.category_id ?? 'other'}-${index}`} className="stats-row">
              <div className="stats-row__head">
                <span className="stats-row__icon" aria-hidden="true">
                  {icon}
                </span>
                <span className="stats-row__label">{label}</span>
                <span className="amount amount--expense">{formatMoney(row.total, currency)}</span>
              </div>
              <div className="stats-row__track">
                <div className="stats-row__fill" style={{ width: `${percent}%` }} />
                <span className="stats-row__pct">{Math.round(percent)}%</span>
              </div>
            </li>
          );
        })}
      </ul>
    </>
  );
}

import { useTranslation } from 'react-i18next';

import type { AccountId } from '../api/types';
import { useMonthBudgets, useMonthSummary } from '../api/hooks';
import { formatMoney, parseMoneyForDisplay } from '../lib/money';
import { ringView } from '../lib/ring';
import { useSelectedMonth, useSettings } from '../state/contexts';
import { BudgetRing } from './BudgetRing';
import { ErrorBanner } from './ErrorBanner';
import { Spinner } from './Spinner';

interface SummaryBarProps {
  accountId: AccountId;
  currency: string;
}

export function SummaryBar({ accountId, currency }: SummaryBarProps) {
  const { t } = useTranslation();
  const { month } = useSelectedMonth();
  const { ringMode } = useSettings();
  const { data: summary, isPending, error } = useMonthSummary(accountId, month);
  // Only to decide whether the "free to spend" figure is meaningful: without a
  // budget this month it would just repeat "available".
  const { data: budgets } = useMonthBudgets(accountId, month);

  if (isPending) {
    return <Spinner label={t('summary.loading')} />;
  }
  if (error) {
    return <ErrorBanner error={error} />;
  }

  const ring = ringView(summary, ringMode);
  const hasBudgets = (budgets?.length ?? 0) > 0;
  // `available` gets its own sign test: `ring.overspent` tracks `free`, which can
  // be negative while `available` is still comfortably positive.
  const availableNegative = parseMoneyForDisplay(summary.available) < 0;

  return (
    <section className="summary">
      <BudgetRing
        spentFraction={ring.fraction}
        overspent={ring.overspent}
        label={t(ring.labelKey, { percent: ring.percent })}
      />
      <dl className="summary__figures">
        <div>
          <dt>{t('summary.income')}</dt>
          <dd className="amount amount--income">{formatMoney(summary.income, currency)}</dd>
        </div>
        <div>
          <dt>{t('summary.expenses')}</dt>
          <dd className="amount amount--expense">{formatMoney(summary.expenses, currency)}</dd>
        </div>
        <div>
          <dt>{t('summary.available')}</dt>
          <dd className={`amount amount--total${availableNegative ? ' amount--negative' : ''}`}>
            {formatMoney(summary.available, currency)}
          </dd>
        </div>
        {hasBudgets && (
          <div>
            <dt>{t('summary.free')}</dt>
            <dd className={`amount amount--total${ring.overspent ? ' amount--negative' : ''}`}>
              {formatMoney(summary.free, currency)}
            </dd>
          </div>
        )}
      </dl>
    </section>
  );
}

import type { AccountId } from '../api/types';
import { useMonthSummary } from '../api/hooks';
import { formatMoney, parseMoneyForDisplay } from '../lib/money';
import { useSelectedMonth } from '../state/contexts';
import { BudgetRing } from './BudgetRing';
import { ErrorBanner } from './ErrorBanner';
import { Spinner } from './Spinner';

interface SummaryBarProps {
  accountId: AccountId;
  currency: string;
}

export function SummaryBar({ accountId, currency }: SummaryBarProps) {
  const { month } = useSelectedMonth();
  const { data: summary, isPending, error } = useMonthSummary(accountId, month);

  if (isPending) {
    return <Spinner label="Loading summary…" />;
  }
  if (error) {
    return <ErrorBanner error={error} />;
  }

  // Display-only ratio: spent vs. funds available before spending.
  const funds = parseMoneyForDisplay(summary.carry_in) + parseMoneyForDisplay(summary.income);
  const spent = parseMoneyForDisplay(summary.expenses);
  const spentFraction = funds > 0 ? spent / funds : 0;
  const overspent = parseMoneyForDisplay(summary.available) < 0;

  return (
    <section className="summary">
      <BudgetRing
        spentFraction={spentFraction}
        overspent={overspent}
        label={`${Math.round(Math.min(Math.max(spentFraction, 0), 1) * 100)}% of this month's funds spent`}
      />
      <dl className="summary__figures">
        <div>
          <dt>Income</dt>
          <dd className="amount amount--income">{formatMoney(summary.income, currency)}</dd>
        </div>
        <div>
          <dt>Expenses</dt>
          <dd className="amount amount--expense">{formatMoney(summary.expenses, currency)}</dd>
        </div>
        <div>
          <dt>Available to end of month</dt>
          <dd className={`amount amount--total${overspent ? ' amount--negative' : ''}`}>
            {formatMoney(summary.available, currency)}
          </dd>
        </div>
      </dl>
    </section>
  );
}

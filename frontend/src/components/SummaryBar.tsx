import { useTranslation } from 'react-i18next';

import type { AccountId, IsoDate } from '../api/types';
import { useBalanceAsOf, useMonthSummary } from '../api/hooks';
import { isoMonthOf } from '../lib/date';
import { formatMoney, parseMoneyForDisplay } from '../lib/money';
import { monthEquals } from '../lib/month';
import { ringView } from '../lib/ring';
import { useSelectedMonth, useSettings, useToday } from '../state/contexts';
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
  // "Balance today" is a point in time, so only the month containing today
  // shows it. `useToday` keeps this current across midnight and app resume, so
  // the row moves on with the day rather than lingering under last month.
  const today = useToday();
  const isCurrentMonth = monthEquals(month, isoMonthOf(today));

  if (isPending) {
    return <Spinner label={t('summary.loading')} />;
  }
  if (error) {
    return <ErrorBanner error={error} />;
  }

  const ring = ringView(summary, ringMode);
  // Show "free to spend" exactly when it differs from "available" — i.e. when
  // something is actually held back. Derived from the summary the ring already
  // uses, rather than a second query: that query's pending/failed states could
  // hide the row while the ring still reflected the reservation, leaving the two
  // contradicting each other. Compare numerically, not `!== '0'`: `Money::zero()`
  // renders as "0" but `planned - actual` at 2dp renders as "0.00".
  const reserved = parseMoneyForDisplay(summary.reserved);
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
        {isCurrentMonth && (
          <BalanceTodayRow accountId={accountId} currency={currency} today={today} />
        )}
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
        {reserved !== 0 && (
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

interface BalanceTodayRowProps {
  accountId: AccountId;
  currency: string;
  today: IsoDate;
}

/**
 * The current month's "Balance today": what the account should hold right now
 * if every transaction was recorded (`docs/DESIGN.md` §13).
 *
 * A secondary figure with its own query, so its loading and failure states stay
 * in this row: the month's figures never wait for it or vanish with it. Unlike
 * "free" above, nothing else in the bar is derived from it, so a moment of
 * disagreement can't leave two figures contradicting each other. The row is laid
 * out while loading, so the bar doesn't jump when the value lands.
 */
function BalanceTodayRow({ accountId, currency, today }: BalanceTodayRowProps) {
  const { t } = useTranslation();
  const { data, isError } = useBalanceAsOf(accountId, today);
  // A failed refetch keeps the last good value in `data`. For a figure meant to
  // be checked against the bank, a dash beats a silently stale number.
  const balance = isError ? undefined : data;
  const negative = balance !== undefined && parseMoneyForDisplay(balance) < 0;
  const placeholder = isError ? '—' : '…';

  return (
    <div className="summary__balance">
      <dt>{t('summary.balanceToday')}</dt>
      <dd className={`amount${negative ? ' amount--negative' : ''}`}>
        {balance === undefined ? placeholder : formatMoney(balance, currency)}
      </dd>
    </div>
  );
}

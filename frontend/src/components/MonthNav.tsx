import { useTranslation } from 'react-i18next';

import { isoMonthOf } from '../lib/date';
import { monthEquals, monthLabel } from '../lib/month';
import { useSelectedMonth, useToday } from '../state/contexts';

export function MonthNav() {
  const { t } = useTranslation();
  const { month, setMonth, next, prev } = useSelectedMonth();
  // From the app's shared clock, so "current" moves on at midnight together
  // with the month bar's "Balance today", not at the next unrelated re-render.
  const current = isoMonthOf(useToday());
  const isCurrent = monthEquals(month, current);
  const label = monthLabel(month);
  return (
    <nav className="month-nav" aria-label={t('month.navigation')}>
      <button type="button" className="month-nav__arrow" onClick={prev} aria-label={t('month.previous')}>
        ‹
      </button>
      {/* The visible label is the button's name; when off-month it also carries
          the jump-to-current hint. (Arrow-driven changes are announced by the
          live region below, since focus stays on the arrow.) */}
      <button
        type="button"
        className="month-nav__label"
        data-current={isCurrent}
        aria-label={isCurrent ? undefined : `${label}, ${t('month.goToCurrent')}`}
        title={isCurrent ? undefined : t('month.goToCurrent')}
        disabled={isCurrent}
        onClick={() => setMonth(current)}
      >
        {label}
      </button>
      <button type="button" className="month-nav__arrow" onClick={next} aria-label={t('month.next')}>
        ›
      </button>
      <span className="visually-hidden" aria-live="polite" aria-atomic="true">
        {label}
      </span>
    </nav>
  );
}

import { useTranslation } from 'react-i18next';

import { currentMonth, monthEquals, monthLabel } from '../lib/month';
import { useSelectedMonth } from '../state/contexts';

export function MonthNav() {
  const { t } = useTranslation();
  const { month, setMonth, next, prev } = useSelectedMonth();
  const isCurrent = monthEquals(month, currentMonth());
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
        onClick={() => setMonth(currentMonth())}
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

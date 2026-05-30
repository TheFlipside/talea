import { useTranslation } from 'react-i18next';

import { monthLabel } from '../lib/month';
import { useSelectedMonth } from '../state/contexts';

export function MonthNav() {
  const { t } = useTranslation();
  const { month, next, prev } = useSelectedMonth();
  return (
    <nav className="month-nav" aria-label={t('month.navigation')}>
      <button type="button" className="month-nav__arrow" onClick={prev} aria-label={t('month.previous')}>
        ‹
      </button>
      <span className="month-nav__label" aria-live="polite">
        {monthLabel(month)}
      </span>
      <button type="button" className="month-nav__arrow" onClick={next} aria-label={t('month.next')}>
        ›
      </button>
    </nav>
  );
}

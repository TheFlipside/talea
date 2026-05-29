import { monthLabel } from '../lib/month';
import { useSelectedMonth } from '../state/contexts';

export function MonthNav() {
  const { month, next, prev } = useSelectedMonth();
  return (
    <nav className="month-nav" aria-label="Month navigation">
      <button type="button" className="month-nav__arrow" onClick={prev} aria-label="Previous month">
        ‹
      </button>
      <span className="month-nav__label" aria-live="polite">
        {monthLabel(month)}
      </span>
      <button type="button" className="month-nav__arrow" onClick={next} aria-label="Next month">
        ›
      </button>
    </nav>
  );
}

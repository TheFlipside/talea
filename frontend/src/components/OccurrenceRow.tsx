import { useTranslation } from 'react-i18next';

import type { Category, Occurrence } from '../api/types';
import { categoryIconText } from '../lib/categories';
import { formatEntryDate } from '../lib/date';
import { formatMoney } from '../lib/money';

interface OccurrenceRowProps {
  occurrence: Occurrence;
  currency: string;
  category?: Category;
  onSelect: (occurrence: Occurrence) => void;
}

/**
 * A month-list row for a recurring-rule occurrence. It is derived from a rule
 * (not a stored entry), marked with a 🔁 badge; tapping it opens actions to
 * edit or remove that single occurrence.
 */
export function OccurrenceRow({ occurrence, currency, category, onSelect }: OccurrenceRowProps) {
  const { t } = useTranslation();
  const income = occurrence.kind === 'income';
  const sign = income ? '+' : '−';
  const title = occurrence.note ?? category?.label ?? t(income ? 'entry.income' : 'entry.expense');

  return (
    <li>
      <button
        type="button"
        className="entry-row entry-row--recurring"
        aria-label={t('recurring.occurrenceAria')}
        onClick={() => onSelect(occurrence)}
      >
        <span className="entry-row__icon" aria-hidden="true">
          {category ? categoryIconText(category.icon) : income ? '＋' : '－'}
        </span>
        <span className="entry-row__main">
          <span className="entry-row__title">
            {title}
            <span className="recurring-badge" title={t('recurring.fromRule')} aria-hidden="true">
              🔁
            </span>
          </span>
          <span className="entry-row__date">
            {category && occurrence.note ? `${category.label} · ` : ''}
            {formatEntryDate(occurrence.date)}
          </span>
        </span>
        <span className={`amount ${income ? 'amount--income' : 'amount--expense'}`}>
          {sign}
          {formatMoney(occurrence.amount, currency)}
        </span>
      </button>
    </li>
  );
}

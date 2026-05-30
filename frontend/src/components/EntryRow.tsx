import { useTranslation } from 'react-i18next';

import type { Category, Entry } from '../api/types';
import { categoryIconText } from '../lib/categories';
import { formatEntryDate } from '../lib/date';
import { formatMoney } from '../lib/money';

interface EntryRowProps {
  entry: Entry;
  currency: string;
  category?: Category;
  onEdit: (entry: Entry) => void;
}

export function EntryRow({ entry, currency, category, onEdit }: EntryRowProps) {
  const { t } = useTranslation();
  const income = entry.kind === 'income';
  const sign = income ? '+' : '−';
  // Primary text: the note, else the category label, else the kind.
  const title = entry.note ?? category?.label ?? t(income ? 'entry.income' : 'entry.expense');

  return (
    <li>
      <button type="button" className="entry-row" onClick={() => onEdit(entry)}>
        <span className="entry-row__icon" aria-hidden="true">
          {category ? categoryIconText(category.icon) : income ? '＋' : '－'}
        </span>
        <span className="entry-row__main">
          <span className="entry-row__title">{title}</span>
          <span className="entry-row__date">
            {category && entry.note ? `${category.label} · ` : ''}
            {formatEntryDate(entry.date)}
          </span>
        </span>
        <span className={`amount ${income ? 'amount--income' : 'amount--expense'}`}>
          {sign}
          {formatMoney(entry.amount, currency)}
        </span>
      </button>
    </li>
  );
}

import { useTranslation } from 'react-i18next';

import type { Category, Entry } from '../api/types';
import type { AccountTag } from './EntryList';
import { categoryIconText } from '../lib/categories';
import { formatEntryDate } from '../lib/date';
import { formatMoney } from '../lib/money';

interface EntryRowProps {
  entry: Entry;
  currency: string;
  category?: Category;
  /** Read-only rows (summary account view) aren't tappable. */
  readOnly?: boolean;
  /** Owning-account label, shown only in the summary view. */
  accountTag?: AccountTag;
  onEdit: (entry: Entry) => void;
}

export function EntryRow({
  entry,
  currency,
  category,
  readOnly = false,
  accountTag,
  onEdit,
}: EntryRowProps) {
  const { t } = useTranslation();
  const income = entry.kind === 'income';
  const sign = income ? '+' : '−';
  // Primary text: the note, else the category label, else the kind.
  const title = entry.note ?? category?.label ?? t(income ? 'entry.income' : 'entry.expense');

  const body = (
    <>
      <span className="entry-row__icon" aria-hidden="true">
        {category ? categoryIconText(category.icon) : income ? '＋' : '－'}
      </span>
      <span className="entry-row__main">
        <span className="entry-row__title">{title}</span>
        <span className="entry-row__date">
          {accountTag && (
            <span className="entry-row__account">
              <span
                className="entry-row__account-dot"
                style={{ background: accountTag.color }}
                aria-hidden="true"
              />
              {accountTag.name} ·{' '}
            </span>
          )}
          {category && entry.note ? `${category.label} · ` : ''}
          {formatEntryDate(entry.date)}
        </span>
      </span>
      <span className={`amount ${income ? 'amount--income' : 'amount--expense'}`}>
        {sign}
        {formatMoney(entry.amount, currency)}
      </span>
    </>
  );

  return (
    <li>
      {readOnly ? (
        <div className="entry-row entry-row--readonly">{body}</div>
      ) : (
        <button type="button" className="entry-row" onClick={() => onEdit(entry)}>
          {body}
        </button>
      )}
    </li>
  );
}

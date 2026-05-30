import { useTranslation } from 'react-i18next';

import type { Entry } from '../api/types';
import { formatEntryDate } from '../lib/date';
import { formatMoney } from '../lib/money';

interface EntryRowProps {
  entry: Entry;
  currency: string;
  onEdit: (entry: Entry) => void;
}

export function EntryRow({ entry, currency, onEdit }: EntryRowProps) {
  const { t } = useTranslation();
  const income = entry.kind === 'income';
  const sign = income ? '+' : '−';
  return (
    <li>
      <button type="button" className="entry-row" onClick={() => onEdit(entry)}>
        <span className="entry-row__main">
          <span className="entry-row__note">
            {entry.note ?? t(income ? 'entry.income' : 'entry.expense')}
          </span>
          <span className="entry-row__date">{formatEntryDate(entry.date)}</span>
        </span>
        <span className={`amount ${income ? 'amount--income' : 'amount--expense'}`}>
          {sign}
          {formatMoney(entry.amount, currency)}
        </span>
      </button>
    </li>
  );
}

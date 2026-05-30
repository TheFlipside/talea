import { useTranslation } from 'react-i18next';

import type { AccountId, Category, Entry } from '../api/types';
import { useCategories, useEntries } from '../api/hooks';
import { filterEntriesToMonth, sortEntriesForDisplay } from '../lib/entries';
import { useSelectedMonth } from '../state/contexts';
import { EntryRow } from './EntryRow';
import { ErrorBanner } from './ErrorBanner';
import { Spinner } from './Spinner';

interface EntryListProps {
  accountId: AccountId;
  currency: string;
  onEdit: (entry: Entry) => void;
}

export function EntryList({ accountId, currency, onEdit }: EntryListProps) {
  const { t } = useTranslation();
  const { month } = useSelectedMonth();
  const { data: entries, isPending, error } = useEntries(accountId);
  const { data: categories } = useCategories();

  if (isPending) {
    return <Spinner label={t('entry.loading')} />;
  }
  if (error) {
    return <ErrorBanner error={error} />;
  }

  const byId = new Map<number, Category>((categories ?? []).map((c) => [c.id, c]));
  const monthEntries = sortEntriesForDisplay(filterEntriesToMonth(entries, month));

  if (monthEntries.length === 0) {
    return <p className="entry-list__empty">{t('entry.empty')}</p>;
  }

  return (
    <ul className="entry-list">
      {monthEntries.map((entry) => (
        <EntryRow
          key={entry.id}
          entry={entry}
          currency={currency}
          category={entry.category_id != null ? byId.get(entry.category_id) : undefined}
          onEdit={onEdit}
        />
      ))}
    </ul>
  );
}

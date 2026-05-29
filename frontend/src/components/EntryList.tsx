import type { AccountId, Entry } from '../api/types';
import { useEntries } from '../api/hooks';
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
  const { month } = useSelectedMonth();
  const { data: entries, isPending, error } = useEntries(accountId);

  if (isPending) {
    return <Spinner label="Loading entries…" />;
  }
  if (error) {
    return <ErrorBanner error={error} />;
  }

  const monthEntries = sortEntriesForDisplay(filterEntriesToMonth(entries, month));

  if (monthEntries.length === 0) {
    return <p className="entry-list__empty">No entries this month.</p>;
  }

  return (
    <ul className="entry-list">
      {monthEntries.map((entry) => (
        <EntryRow key={entry.id} entry={entry} currency={currency} onEdit={onEdit} />
      ))}
    </ul>
  );
}

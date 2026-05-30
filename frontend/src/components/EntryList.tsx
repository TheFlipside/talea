import { useTranslation } from 'react-i18next';

import type { AccountId, Category, Entry } from '../api/types';
import { useCategories, useEntries, useMonthOccurrences } from '../api/hooks';
import { filterEntriesToMonth } from '../lib/entries';
import { mergeMonthItems } from '../lib/monthItems';
import { useSelectedMonth } from '../state/contexts';
import { EntryRow } from './EntryRow';
import { ErrorBanner } from './ErrorBanner';
import { OccurrenceRow } from './OccurrenceRow';
import { Spinner } from './Spinner';

interface EntryListProps {
  accountId: AccountId;
  currency: string;
  onEdit: (entry: Entry) => void;
}

export function EntryList({ accountId, currency, onEdit }: EntryListProps) {
  const { t } = useTranslation();
  const { month } = useSelectedMonth();
  const entries = useEntries(accountId);
  const occurrences = useMonthOccurrences(accountId, month);
  const { data: categories } = useCategories();

  if (entries.isPending || occurrences.isPending) {
    return <Spinner label={t('entry.loading')} />;
  }
  const error = entries.error ?? occurrences.error;
  if (error) {
    return <ErrorBanner error={error} />;
  }

  const byId = new Map<number, Category>((categories ?? []).map((c) => [c.id, c]));
  const category = (id?: number | null) => (id != null ? byId.get(id) : undefined);
  // Stored entries (this month) and rule occurrences merged into one ordered list
  // (data is present past the isPending guard; `?? []` just satisfies the types).
  const items = mergeMonthItems(
    filterEntriesToMonth(entries.data ?? [], month),
    occurrences.data ?? [],
  );

  if (items.length === 0) {
    return <p className="entry-list__empty">{t('entry.empty')}</p>;
  }

  return (
    <ul className="entry-list">
      {items.map((item) =>
        item.kind === 'entry' ? (
          <EntryRow
            key={`e${item.entry.id}`}
            entry={item.entry}
            currency={currency}
            category={category(item.entry.category_id)}
            onEdit={onEdit}
          />
        ) : (
          <OccurrenceRow
            key={`o${item.occurrence.rule_id}-${item.occurrence.date}`}
            occurrence={item.occurrence}
            currency={currency}
            category={category(item.occurrence.category_id)}
          />
        ),
      )}
    </ul>
  );
}

import { useTranslation } from 'react-i18next';

import type { Account, AccountId, Category, Entry, Occurrence } from '../api/types';
import { useAccounts, useCategories, useEntries, useMonthOccurrences } from '../api/hooks';
import { accountColor } from '../lib/accountColor';
import { filterEntriesToMonth } from '../lib/entries';
import { mergeMonthItems } from '../lib/monthItems';
import { useSelectedMonth } from '../state/contexts';
import { EntryRow } from './EntryRow';
import { ErrorBanner } from './ErrorBanner';
import { OccurrenceRow } from './OccurrenceRow';
import { Spinner } from './Spinner';

/** A row's owning-account tag, shown only in a summary account's combined view. */
export interface AccountTag {
  name: string;
  color: string;
}

interface EntryListProps {
  accountId: AccountId;
  currency: string;
  /** True for a summary account: rows are read-only and tagged by source account. */
  readOnly?: boolean;
  onEdit: (entry: Entry) => void;
  onSelectOccurrence: (occurrence: Occurrence) => void;
}

export function EntryList({
  accountId,
  currency,
  readOnly = false,
  onEdit,
  onSelectOccurrence,
}: EntryListProps) {
  const { t } = useTranslation();
  const { month } = useSelectedMonth();
  const entries = useEntries(accountId);
  const occurrences = useMonthOccurrences(accountId, month);
  const { data: categories } = useCategories();
  // Only needed to label rows by source account in the summary (read-only) view.
  const { data: accounts } = useAccounts();

  if (entries.isPending || occurrences.isPending) {
    return <Spinner label={t('entry.loading')} />;
  }
  const error = entries.error ?? occurrences.error;
  if (error) {
    return <ErrorBanner error={error} />;
  }

  const byId = new Map<number, Category>((categories ?? []).map((c) => [c.id, c]));
  const category = (id?: number | null) => (id != null ? byId.get(id) : undefined);
  const accountById = new Map<AccountId, Account>((accounts ?? []).map((a) => [a.id, a]));
  // In the summary view, tag each row with the member account it came from.
  const tagFor = (id: AccountId): AccountTag | undefined => {
    if (!readOnly) {
      return undefined;
    }
    const owner = accountById.get(id);
    return owner ? { name: owner.name, color: accountColor(owner.id) } : undefined;
  };
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
            readOnly={readOnly}
            accountTag={tagFor(item.entry.account_id)}
            onEdit={onEdit}
          />
        ) : (
          <OccurrenceRow
            key={`o${item.occurrence.rule_id}-${item.occurrence.date}`}
            occurrence={item.occurrence}
            currency={currency}
            category={category(item.occurrence.category_id)}
            readOnly={readOnly}
            accountTag={tagFor(item.occurrence.account_id)}
            onSelect={onSelectOccurrence}
          />
        ),
      )}
    </ul>
  );
}

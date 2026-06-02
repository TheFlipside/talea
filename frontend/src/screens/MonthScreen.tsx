import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { Account, Entry, Occurrence } from '../api/types';
import { EntryForm } from '../components/EntryForm';
import { EntryList } from '../components/EntryList';
import { MonthNav } from '../components/MonthNav';
import { OccurrenceActions } from '../components/OccurrenceActions';
import { SummaryBar } from '../components/SummaryBar';
import { useSwipe } from '../lib/swipe';
import { useSelectedMonth } from '../state/contexts';

/** A form is open either to create (`'new'`) or to edit a specific entry. */
type FormState = { mode: 'closed' } | { mode: 'new' } | { mode: 'edit'; entry: Entry };

export function MonthScreen({ account }: { account: Account }) {
  const { t } = useTranslation();
  const { next, prev } = useSelectedMonth();
  const [form, setForm] = useState<FormState>({ mode: 'closed' });
  // A recurring occurrence whose actions (edit/remove) are open.
  const [occurrence, setOccurrence] = useState<Occurrence | null>(null);

  // Swipe left → next month, right → previous (natural paging direction).
  const swipe = useSwipe({ onSwipeLeft: next, onSwipeRight: prev });

  // A summary account is a read-only overview: its rows belong to its member
  // accounts, so it takes no records of its own (no + button, no editing).
  const isSummary = account.kind === 'summary';

  return (
    <div className="month-screen" {...swipe}>
      <MonthNav />
      <SummaryBar accountId={account.id} currency={account.currency} />

      {isSummary && <p className="month-screen__summary-hint muted">{t('summary.combinedHint')}</p>}

      <div className="month-screen__list">
        <EntryList
          accountId={account.id}
          currency={account.currency}
          readOnly={isSummary}
          onEdit={(entry) => setForm({ mode: 'edit', entry })}
          onSelectOccurrence={setOccurrence}
        />
      </div>

      {!isSummary && (
        <button
          type="button"
          className="fab"
          aria-label={t('entry.new')}
          onClick={() => setForm({ mode: 'new' })}
        >
          +
        </button>
      )}

      {!isSummary && form.mode !== 'closed' && (
        <EntryForm
          key={form.mode === 'edit' ? form.entry.id : 'new'}
          accountId={account.id}
          currency={account.currency}
          editing={form.mode === 'edit' ? form.entry : null}
          onClose={() => setForm({ mode: 'closed' })}
        />
      )}

      {!isSummary && occurrence && (
        <OccurrenceActions
          accountId={account.id}
          occurrence={occurrence}
          onClose={() => setOccurrence(null)}
          onDetached={(entry) => {
            // The occurrence is now an editable standalone entry — open its editor.
            setOccurrence(null);
            setForm({ mode: 'edit', entry });
          }}
        />
      )}
    </div>
  );
}

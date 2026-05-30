import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { Account, Entry } from '../api/types';
import { EntryForm } from '../components/EntryForm';
import { EntryList } from '../components/EntryList';
import { MonthNav } from '../components/MonthNav';
import { SummaryBar } from '../components/SummaryBar';
import { useSwipe } from '../lib/swipe';
import { useSelectedMonth } from '../state/contexts';

/** A form is open either to create (`'new'`) or to edit a specific entry. */
type FormState = { mode: 'closed' } | { mode: 'new' } | { mode: 'edit'; entry: Entry };

export function MonthScreen({ account }: { account: Account }) {
  const { t } = useTranslation();
  const { next, prev } = useSelectedMonth();
  const [form, setForm] = useState<FormState>({ mode: 'closed' });

  // Swipe left → next month, right → previous (natural paging direction).
  const swipe = useSwipe({ onSwipeLeft: next, onSwipeRight: prev });

  return (
    <div className="month-screen" {...swipe}>
      <MonthNav />
      <SummaryBar accountId={account.id} currency={account.currency} />

      <div className="month-screen__list">
        <EntryList
          accountId={account.id}
          currency={account.currency}
          onEdit={(entry) => setForm({ mode: 'edit', entry })}
        />
      </div>

      <button
        type="button"
        className="fab"
        aria-label={t('entry.new')}
        onClick={() => setForm({ mode: 'new' })}
      >
        +
      </button>

      {form.mode !== 'closed' && (
        <EntryForm
          accountId={account.id}
          currency={account.currency}
          editing={form.mode === 'edit' ? form.entry : null}
          onClose={() => setForm({ mode: 'closed' })}
        />
      )}
    </div>
  );
}

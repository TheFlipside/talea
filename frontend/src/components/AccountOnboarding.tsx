import { useState } from 'react';

import { useCreateAccount } from '../api/hooks';
import { currentMonth } from '../lib/month';
import { useActiveAccount } from '../state/contexts';

interface AccountOnboardingProps {
  /** Show a cancel button (when adding an account, not on first run). */
  allowCancel?: boolean;
  onDone?: () => void;
}

export function AccountOnboarding({ allowCancel = false, onDone }: AccountOnboardingProps) {
  const create = useCreateAccount();
  const { setActiveAccountId } = useActiveAccount();

  const [name, setName] = useState('');
  const [icon, setIcon] = useState('💰');
  const [currency, setCurrency] = useState('USD');
  const [openingBalance, setOpeningBalance] = useState('0.00');

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    create.mutate(
      {
        name: name.trim(),
        icon: icon.trim(),
        currency: currency.trim().toUpperCase(),
        opening_balance: openingBalance.trim(),
        anchor: currentMonth(),
      },
      {
        onSuccess: (account) => {
          setActiveAccountId(account.id);
          onDone?.();
        },
      },
    );
  }

  return (
    <form className="card account-form" onSubmit={handleSubmit}>
      <h2>{allowCancel ? 'New account' : 'Welcome to Talea'}</h2>
        {!allowCancel && <p className="muted">Create an account to start tracking your budget.</p>}

        <label className="field">
          <span>Name</span>
          <input value={name} onChange={(e) => setName(e.currentTarget.value)} placeholder="Checking" required autoFocus />
        </label>

        <div className="field-row">
          <label className="field field--narrow">
            <span>Icon</span>
            <input value={icon} onChange={(e) => setIcon(e.currentTarget.value)} maxLength={8} />
          </label>
          <label className="field field--narrow">
            <span>Currency</span>
            <input
              value={currency}
              onChange={(e) => setCurrency(e.currentTarget.value.toUpperCase())}
              maxLength={3}
              required
            />
          </label>
        </div>

        <label className="field">
          <span>Opening balance ({currency || '—'})</span>
          <input
            inputMode="decimal"
            value={openingBalance}
            onChange={(e) => setOpeningBalance(e.currentTarget.value)}
            required
          />
        </label>

        {create.error && <p className="field-error">{create.error.message}</p>}

        <div className="modal__actions">
          <span className="modal__spacer" />
          {allowCancel && (
            <button type="button" className="btn btn--ghost" onClick={onDone} disabled={create.isPending}>
              Cancel
            </button>
          )}
          <button type="submit" className="btn" disabled={create.isPending}>
            Create account
          </button>
        </div>
    </form>
  );
}

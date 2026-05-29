import { useState } from 'react';

import { useCreateAccount } from '../api/hooks';
import { COMMON_CURRENCIES, currencySymbol } from '../lib/currencies';
import { defaultCurrency } from '../lib/locale';
import { isMoneyInput } from '../lib/money';
import { currentMonth } from '../lib/month';
import { useActiveAccount } from '../state/contexts';
import { Select } from './Select';

const CURRENCY_OPTIONS = COMMON_CURRENCIES.map((c) => {
  const head = (
    <span className="currency-option__head">
      <span className="currency-option__symbol">{currencySymbol(c.code)}</span>
      <span className="currency-option__code">{c.code}</span>
    </span>
  );
  return {
    value: c.code,
    // Two rows in the list: "$ USD" over "US Dollar".
    label: (
      <span className="currency-option">
        {head}
        <span className="currency-option__name">{c.name}</span>
      </span>
    ),
    // Compact single line for the closed trigger.
    triggerLabel: head,
  };
});

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
  const [currency, setCurrency] = useState(defaultCurrency);
  const [openingBalance, setOpeningBalance] = useState('0.00');
  const [localError, setLocalError] = useState<string | null>(null);

  const errorMessage = localError ?? create.error?.message ?? null;

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const trimmedName = name.trim();
    const code = currency.trim().toUpperCase();
    const balance = openingBalance.trim() === '' ? '0' : openingBalance.trim();

    if (trimmedName === '') {
      setLocalError('Please enter a name.');
      return;
    }
    if (!/^[A-Z]{3}$/.test(code)) {
      setLocalError('Currency must be a 3-letter code, e.g. USD.');
      return;
    }
    if (!isMoneyInput(balance)) {
      setLocalError('Enter a valid opening balance, e.g. 0.00.');
      return;
    }
    setLocalError(null);

    create.mutate(
      {
        name: trimmedName,
        icon: icon.trim() === '' ? '💰' : icon.trim(),
        currency: code,
        opening_balance: balance,
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
          <div className="field field--narrow">
            <span>Currency</span>
            <Select
              value={currency}
              onChange={setCurrency}
              options={CURRENCY_OPTIONS}
              ariaLabel="Currency"
            />
          </div>
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

        {errorMessage && <p className="field-error">{errorMessage}</p>}

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

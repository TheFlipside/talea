import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { Account } from '../api/types';
import { useCreateAccount, useUpdateAccount } from '../api/hooks';
import { COMMON_CURRENCIES, currencySymbol } from '../lib/currencies';
import { defaultCurrency } from '../lib/locale';
import { isMoneyInput } from '../lib/money';
import { currentMonth, monthLabel } from '../lib/month';
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
    label: (
      <span className="currency-option">
        {head}
        <span className="currency-option__name">{c.name}</span>
      </span>
    ),
    triggerLabel: head,
  };
});

interface AccountFormProps {
  mode: 'create' | 'edit';
  /** The account being edited (required when `mode === 'edit'`). */
  account?: Account;
  onDone: () => void;
}

export function AccountForm({ mode, account, onDone }: AccountFormProps) {
  const { t } = useTranslation();
  const create = useCreateAccount();
  const update = useUpdateAccount();
  const { setActiveAccountId } = useActiveAccount();

  const [name, setName] = useState(account?.name ?? '');
  const [icon, setIcon] = useState(account?.icon ?? '💰');
  const [currency, setCurrency] = useState(account?.currency ?? defaultCurrency());
  const [openingBalance, setOpeningBalance] = useState(account?.opening_balance ?? '0.00');
  const [localError, setLocalError] = useState<string | null>(null);

  const anchor = account?.anchor ?? currentMonth();
  const errorMessage = localError ?? create.error?.message ?? update.error?.message ?? null;
  const busy = create.isPending || update.isPending;

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const trimmedName = name.trim();
    const code = currency.trim().toUpperCase();
    const balance = openingBalance.trim() === '' ? '0' : openingBalance.trim();

    if (trimmedName === '') {
      setLocalError(t('account.errorName'));
      return;
    }
    if (!/^[A-Z]{3}$/.test(code)) {
      setLocalError(t('account.errorCurrency'));
      return;
    }
    if (!isMoneyInput(balance)) {
      setLocalError(t('account.errorBalance'));
      return;
    }
    setLocalError(null);

    const cleanIcon = icon.trim() === '' ? '💰' : icon.trim();
    if (mode === 'edit' && account) {
      update.mutate(
        {
          id: account.id,
          name: trimmedName,
          icon: cleanIcon,
          currency: code,
          opening_balance: balance,
          anchor,
        },
        { onSuccess: onDone },
      );
    } else {
      create.mutate(
        { name: trimmedName, icon: cleanIcon, currency: code, opening_balance: balance, anchor },
        {
          onSuccess: (created) => {
            setActiveAccountId(created.id);
            onDone();
          },
        },
      );
    }
  }

  return (
    <form className="card account-form" onSubmit={handleSubmit}>
      <h2>{mode === 'edit' ? t('account.edit') : t('account.new')}</h2>

      <label className="field">
        <span>{t('account.name')}</span>
        <input
          value={name}
          onChange={(e) => setName(e.currentTarget.value)}
          placeholder={t('account.namePlaceholder')}
          required
          autoFocus
        />
      </label>

      <div className="field-row">
        <label className="field field--narrow">
          <span>{t('account.icon')}</span>
          <input value={icon} onChange={(e) => setIcon(e.currentTarget.value)} maxLength={8} />
        </label>
        <div className="field field--narrow">
          <span>{t('account.currency')}</span>
          <Select
            value={currency}
            onChange={setCurrency}
            options={CURRENCY_OPTIONS}
            ariaLabel={t('account.currency')}
          />
        </div>
      </div>

      {mode === 'edit' && account && currency !== account.currency && (
        <p className="field-warning">{t('account.currencyChangeWarning')}</p>
      )}

      <label className="field">
        <span>{t('account.openingBalance', { currency: currency || '—' })}</span>
        <input
          inputMode="decimal"
          value={openingBalance}
          onChange={(e) => setOpeningBalance(e.currentTarget.value)}
          required
        />
      </label>

      {mode === 'edit' && (
        <p className="muted account-form__anchor">
          {t('account.startMonth')}: {monthLabel(anchor)}
        </p>
      )}

      {errorMessage && <p className="field-error">{errorMessage}</p>}

      <div className="modal__actions">
        <span className="modal__spacer" />
        <button type="button" className="btn btn--ghost" onClick={onDone} disabled={busy}>
          {t('common.cancel')}
        </button>
        <button type="submit" className="btn" disabled={busy}>
          {mode === 'edit' ? t('account.save') : t('account.create')}
        </button>
      </div>
    </form>
  );
}

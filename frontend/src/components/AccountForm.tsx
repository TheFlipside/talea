import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { Account, AccountId, AccountKind } from '../api/types';
import { useAccounts, useCreateAccount, useUpdateAccount } from '../api/hooks';
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
  const accounts = useAccounts();
  const { setActiveAccountId } = useActiveAccount();

  const [name, setName] = useState(account?.name ?? '');
  const [icon, setIcon] = useState(account?.icon ?? '💰');
  const [currency, setCurrency] = useState(account?.currency ?? defaultCurrency());
  const [openingBalance, setOpeningBalance] = useState(account?.opening_balance ?? '0.00');
  const [kind, setKind] = useState<AccountKind>(account?.kind ?? 'normal');
  const [members, setMembers] = useState<AccountId[]>(account?.members ?? []);
  const [localError, setLocalError] = useState<string | null>(null);

  const anchor = account?.anchor ?? currentMonth();
  const errorMessage = localError ?? create.error?.message ?? update.error?.message ?? null;
  const busy = create.isPending || update.isPending;
  const isSummary = kind === 'summary';
  // A summary aggregates same-currency *normal* accounts (never itself).
  const code = currency.trim().toUpperCase();
  const memberCandidates = (accounts.data ?? []).filter(
    (a) => a.kind === 'normal' && a.currency === code && a.id !== account?.id,
  );

  const kindOptions = [
    { value: 'normal', label: t('account.kindNormal') },
    { value: 'summary', label: t('account.kindSummary') },
  ];

  function toggleMember(id: AccountId) {
    setMembers((prev) => (prev.includes(id) ? prev.filter((m) => m !== id) : [...prev, id]));
  }

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const trimmedName = name.trim();
    const balance = openingBalance.trim() === '' ? '0' : openingBalance.trim();

    if (trimmedName === '') {
      setLocalError(t('account.errorName'));
      return;
    }
    if (!/^[A-Z]{3}$/.test(code)) {
      setLocalError(t('account.errorCurrency'));
      return;
    }
    // Keep only members that still match the chosen currency (a currency change
    // can strand earlier selections).
    const validMembers = members.filter((id) => memberCandidates.some((a) => a.id === id));
    if (isSummary) {
      if (validMembers.length === 0) {
        setLocalError(t('account.errorMembers'));
        return;
      }
    } else if (!isMoneyInput(balance)) {
      setLocalError(t('account.errorBalance'));
      return;
    }
    setLocalError(null);

    const cleanIcon = icon.trim() === '' ? '💰' : icon.trim();
    // A summary holds no balance of its own; a normal account has no members.
    const payload = {
      name: trimmedName,
      icon: cleanIcon,
      currency: code,
      opening_balance: isSummary ? '0' : balance,
      anchor,
      kind,
      members: isSummary ? validMembers : [],
    };
    if (mode === 'edit' && account) {
      update.mutate({ id: account.id, ...payload }, { onSuccess: onDone });
    } else {
      create.mutate(payload, {
        onSuccess: (created) => {
          setActiveAccountId(created.id);
          onDone();
        },
      });
    }
  }

  return (
    <form className="card account-form" onSubmit={handleSubmit}>
      <h2>{mode === 'edit' ? t('account.edit') : t('account.new')}</h2>

      <div className="field">
        <span>{t('account.kind')}</span>
        <Select
          value={kind}
          onChange={(v) => setKind(v as AccountKind)}
          options={kindOptions}
          ariaLabel={t('account.kind')}
          // An account's type is fixed once created.
          disabled={mode === 'edit'}
        />
      </div>

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

      {isSummary ? (
        <div className="field">
          <span>{t('account.members')}</span>
          <p className="muted account-form__members-hint">{t('account.membersHint')}</p>
          {memberCandidates.length === 0 ? (
            <p className="field-warning">{t('account.membersEmpty', { currency: code })}</p>
          ) : (
            <ul className="account-form__members">
              {memberCandidates.map((a) => (
                <li key={a.id}>
                  <label className="account-form__member">
                    <input
                      type="checkbox"
                      checked={members.includes(a.id)}
                      onChange={() => toggleMember(a.id)}
                    />
                    <span>
                      {a.icon} {a.name}
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          )}
        </div>
      ) : (
        <label className="field">
          <span>{t('account.openingBalance', { currency: currency || '—' })}</span>
          <input
            inputMode="decimal"
            value={openingBalance}
            onChange={(e) => setOpeningBalance(e.currentTarget.value)}
            required
          />
        </label>
      )}

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

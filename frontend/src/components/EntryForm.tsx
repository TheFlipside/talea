import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { AccountId, Entry, EntryKind } from '../api/types';
import {
  useAccounts,
  useCategories,
  useCreateEntry,
  useCreateTransfer,
  useDeleteEntry,
  useUpdateEntry,
} from '../api/hooks';
import { categoryIconText } from '../lib/categories';
import { defaultDateForMonth } from '../lib/date';
import { isMoneyInput } from '../lib/money';
import { useSelectedMonth } from '../state/contexts';
import { DatePicker } from './DatePicker';
import { Modal } from './Modal';
import { Select } from './Select';

interface EntryFormProps {
  accountId: AccountId;
  currency: string;
  /** The entry being edited, or `null` to create a new one. */
  editing: Entry | null;
  onClose: () => void;
}

export function EntryForm({ accountId, currency, editing, onClose }: EntryFormProps) {
  const { t } = useTranslation();
  const { month } = useSelectedMonth();
  const create = useCreateEntry(accountId);
  const update = useUpdateEntry(accountId);
  const remove = useDeleteEntry(accountId);
  const transfer = useCreateTransfer(accountId);
  const { data: categories } = useCategories();
  const { data: accounts } = useAccounts();

  const [amount, setAmount] = useState(editing?.amount ?? '');
  const [kind, setKind] = useState<EntryKind>(editing?.kind ?? 'expense');
  const [date, setDate] = useState(editing?.date ?? defaultDateForMonth(month));
  const [note, setNote] = useState(editing?.note ?? '');
  const [categoryId, setCategoryId] = useState(
    editing?.category_id != null ? String(editing.category_id) : '',
  );

  // Transfer (new entries only): mirror this entry onto another same-currency
  // account as the opposite kind. No FX, so only same-currency accounts qualify.
  const transferTargets = (accounts ?? []).filter(
    (a) => a.id !== accountId && a.currency === currency,
  );
  const canTransfer = !editing && transferTargets.length > 0;
  const [transferOn, setTransferOn] = useState(false);
  const [counterId, setCounterId] = useState('');
  // Resolve the selected target by matching ids (never Number()-parse), falling
  // back to the first eligible account when nothing is picked yet.
  const counterAccountId =
    transferTargets.find((a) => String(a.id) === counterId)?.id ?? transferTargets[0]?.id;

  const [localError, setLocalError] = useState<string | null>(null);

  const categoryOptions = [
    { value: '', label: t('entry.categoryNone') },
    ...(categories ?? []).map((c) => ({
      value: String(c.id),
      label: (
        <span>
          {categoryIconText(c.icon)} {c.label}
        </span>
      ),
    })),
  ];

  const mutationError = create.error ?? update.error ?? remove.error ?? transfer.error;
  const errorMessage = localError ?? mutationError?.message ?? null;
  const busy = create.isPending || update.isPending || remove.isPending || transfer.isPending;

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const trimmedAmount = amount.trim();
    if (!isMoneyInput(trimmedAmount) || Number(trimmedAmount) <= 0) {
      setLocalError(t('entry.invalidAmount'));
      return;
    }
    setLocalError(null);
    const trimmedNote = note.trim();
    const noteValue = trimmedNote === '' ? null : trimmedNote;
    const categoryValue = categoryId === '' ? null : Number(categoryId);
    if (editing) {
      update.mutate(
        { ...editing, amount: amount.trim(), kind, date, note: noteValue, category_id: categoryValue },
        { onSuccess: onClose },
      );
      return;
    }
    const payload = {
      account_id: accountId,
      amount: amount.trim(),
      kind,
      date,
      note: noteValue,
      category_id: categoryValue,
    };
    if (transferOn && canTransfer && counterAccountId != null) {
      // Mirror onto the other account (opposite kind) in one atomic transfer.
      transfer.mutate({ entry: payload, counterAccountId }, { onSuccess: onClose });
    } else {
      create.mutate(payload, { onSuccess: onClose });
    }
  }

  return (
    <Modal label={editing ? t('entry.edit') : t('entry.new')} onClose={onClose}>
      <form className="entry-form" onSubmit={handleSubmit}>
        <h2>{editing ? t('entry.edit') : t('entry.new')}</h2>

        <div className="segmented" role="group" aria-label={t('entry.kind')}>
          {(['expense', 'income'] as const).map((k) => (
            <button
              key={k}
              type="button"
              className={`segmented__option${kind === k ? ' segmented__option--active' : ''}`}
              aria-pressed={kind === k}
              onClick={() => setKind(k)}
            >
              {k === 'expense' ? t('entry.expense') : t('entry.income')}
            </button>
          ))}
        </div>

        <label className="field">
          <span>{t('entry.amount', { currency })}</span>
          <input
            inputMode="decimal"
            value={amount}
            onChange={(e) => setAmount(e.currentTarget.value)}
            placeholder={t('entry.amountPlaceholder')}
            required
            data-autofocus="true"
          />
        </label>

        <div className="field">
          <span>{t('entry.date')}</span>
          <DatePicker value={date} onChange={setDate} ariaLabel={t('entry.date')} />
        </div>

        <div className="field">
          <span>{t('entry.category')}</span>
          <Select
            value={categoryId}
            options={categoryOptions}
            onChange={setCategoryId}
            ariaLabel={t('entry.category')}
          />
        </div>

        <label className="field">
          <span>{t('entry.note')}</span>
          <input
            value={note}
            onChange={(e) => setNote(e.currentTarget.value)}
            placeholder={t('entry.notePlaceholder')}
          />
        </label>

        {canTransfer && (
          <div className="field">
            <label className="transfer-toggle">
              <input
                type="checkbox"
                checked={transferOn}
                onChange={(e) => setTransferOn(e.currentTarget.checked)}
              />
              <span>{kind === 'expense' ? t('entry.transferToIncome') : t('entry.transferToExpense')}</span>
            </label>
            {transferOn && (
              <Select
                value={counterAccountId != null ? String(counterAccountId) : ''}
                options={transferTargets.map((a) => ({
                  value: String(a.id),
                  label: (
                    <span>
                      {a.icon} {a.name}
                    </span>
                  ),
                }))}
                onChange={setCounterId}
                ariaLabel={t('entry.transferAccount')}
              />
            )}
          </div>
        )}

        {errorMessage && <p className="field-error">{errorMessage}</p>}

        <div className="modal__actions">
          {editing && (
            <button
              type="button"
              className="btn btn--danger"
              disabled={busy}
              onClick={() => {
                remove.mutate(editing.id, { onSuccess: onClose });
              }}
            >
              {t('entry.delete')}
            </button>
          )}
          <span className="modal__spacer" />
          <button type="button" className="btn btn--ghost" onClick={onClose} disabled={busy}>
            {t('common.cancel')}
          </button>
          <button type="submit" className="btn" disabled={busy}>
            {editing ? t('entry.save') : t('entry.add')}
          </button>
        </div>
      </form>
    </Modal>
  );
}

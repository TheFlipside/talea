import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type {
  AccountId,
  AmountSegment,
  EntryKind,
  FreqUnit,
  RecurringRule,
  RuleEnd,
} from '../api/types';
import { useCategories, useCreateRule, useUpdateRule } from '../api/hooks';
import { categoryIconText } from '../lib/categories';
import { formatFullDate, isoDate, todayISO } from '../lib/date';
import { currentMonth } from '../lib/month';
import { formatMoney, isMoneyInput } from '../lib/money';
import { DatePicker } from './DatePicker';
import { Modal } from './Modal';
import { Select } from './Select';

interface RuleFormProps {
  accountId: AccountId;
  currency: string;
  /** The rule being edited, or `null` to create a new one. */
  editing: RecurringRule | null;
  onClose: () => void;
}

/** How an edited amount applies: to every month, or from an effective date on. */
type AmountScope = 'all' | 'from';

const firstOfCurrentMonth = (): string => {
  const m = currentMonth();
  return isoDate(m.year, m.month, 1);
};

export function RuleForm({ accountId, currency, editing, onClose }: RuleFormProps) {
  const { t } = useTranslation();
  const create = useCreateRule(accountId);
  const update = useUpdateRule(accountId);
  const { data: categories } = useCategories();

  const latestAmount = editing ? editing.amounts[editing.amounts.length - 1].amount : '';
  const [amount, setAmount] = useState(latestAmount);
  const [kind, setKind] = useState<EntryKind>(editing?.kind ?? 'expense');
  const [startDate, setStartDate] = useState(editing?.start_date ?? todayISO());
  const [note, setNote] = useState(editing?.note ?? '');
  const [categoryId, setCategoryId] = useState(
    editing?.category_id != null ? String(editing.category_id) : '',
  );
  const [unit, setUnit] = useState<FreqUnit>(editing?.frequency.unit ?? 'monthly');
  const [interval, setInterval] = useState(String(editing?.frequency.interval ?? 1));
  const [endKind, setEndKind] = useState<RuleEnd['kind']>(editing?.end.kind ?? 'never');
  const [endDate, setEndDate] = useState(
    editing?.end.kind === 'until' ? editing.end.date : firstOfCurrentMonth(),
  );
  // Amount-change scope (edit only): default to "from this month" so a raise
  // never rewrites the past, which would alter historical carry-over balances.
  const [amountScope, setAmountScope] = useState<AmountScope>('from');
  const [effectiveFrom, setEffectiveFrom] = useState(firstOfCurrentMonth());
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
  const unitOptions: { value: FreqUnit; label: string }[] = [
    { value: 'weekly', label: t('recurring.unitWeekly') },
    { value: 'monthly', label: t('recurring.unitMonthly') },
    { value: 'yearly', label: t('recurring.unitYearly') },
  ];

  const mutationError = create.error ?? update.error;
  const errorMessage = localError ?? mutationError?.message ?? null;
  const busy = create.isPending || update.isPending;
  const hasHistory = editing != null && editing.amounts.length > 1;

  /**
   * The amount history to persist on an edit. The base segment is always
   * re-anchored to the (possibly changed) `startDate` so the domain's "first
   * segment at start_date" invariant holds, and breakpoints are kept strictly
   * between the start and the effective date so the list stays ascending/unique:
   *   - amount unchanged → keep the base amount + breakpoints still after start;
   *   - "all months"     → a single base segment at the new amount;
   *   - "from"           → base (old amount) + earlier breakpoints, then the new
   *                        amount from the effective date (dropping later ones).
   */
  function nextAmounts(rule: RecurringRule, trimmed: string): AmountSegment[] {
    const base = rule.amounts[0];
    const breakpoints = rule.amounts.slice(1);
    // Numeric compare (not string) so "1200" vs the stored "1200.00" reads as
    // unchanged; this is only a control decision, never a stored money value, and
    // amounts are bounded well within float precision by the core's amount cap.
    const changed = Number(trimmed) !== Number(latestAmount);

    if (!changed) {
      return [
        { effective_from: startDate, amount: base.amount },
        ...breakpoints.filter((s) => s.effective_from > startDate),
      ];
    }
    if (amountScope === 'all' || effectiveFrom <= startDate) {
      return [{ effective_from: startDate, amount: trimmed }];
    }
    return [
      { effective_from: startDate, amount: base.amount },
      ...breakpoints.filter((s) => s.effective_from > startDate && s.effective_from < effectiveFrom),
      { effective_from: effectiveFrom, amount: trimmed },
    ];
  }

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const trimmed = amount.trim();
    if (!isMoneyInput(trimmed) || Number(trimmed) <= 0) {
      setLocalError(t('entry.invalidAmount'));
      return;
    }
    const n = Number(interval);
    if (!Number.isInteger(n) || n < 1) {
      setLocalError(t('recurring.invalidInterval'));
      return;
    }
    const end: RuleEnd = endKind === 'never' ? { kind: 'never' } : { kind: 'until', date: endDate };
    if (end.kind === 'until' && end.date < startDate) {
      setLocalError(t('recurring.endBeforeStart'));
      return;
    }
    setLocalError(null);

    const noteValue = note.trim() === '' ? null : note.trim();
    const categoryValue = categoryId === '' ? null : Number(categoryId);
    const frequency = { unit, interval: n };

    if (editing) {
      update.mutate(
        {
          ...editing,
          amounts: nextAmounts(editing, trimmed),
          kind,
          note: noteValue,
          category_id: categoryValue,
          start_date: startDate,
          end,
          frequency,
        },
        { onSuccess: onClose },
      );
    } else {
      create.mutate(
        {
          account_id: accountId,
          amount: trimmed,
          kind,
          note: noteValue,
          category_id: categoryValue,
          start_date: startDate,
          end,
          frequency,
        },
        { onSuccess: onClose },
      );
    }
  }

  return (
    <Modal label={editing ? t('recurring.edit') : t('recurring.new')} onClose={onClose}>
      <form className="entry-form" onSubmit={handleSubmit}>
        <h2>{editing ? t('recurring.edit') : t('recurring.new')}</h2>

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

        {hasHistory && (
          <div className="rule-history">
            <span className="muted">{t('recurring.amountHistory')}</span>
            <ul>
              {editing.amounts.map((s) => (
                <li key={s.effective_from}>
                  {formatMoney(s.amount, currency)} ·{' '}
                  {t('recurring.fromDate', { date: formatFullDate(s.effective_from) })}
                </li>
              ))}
            </ul>
          </div>
        )}

        <label className="field">
          <span>{editing ? t('recurring.amountCurrent', { currency }) : t('entry.amount', { currency })}</span>
          <input
            inputMode="decimal"
            value={amount}
            onChange={(e) => setAmount(e.currentTarget.value)}
            placeholder={t('entry.amountPlaceholder')}
            required
            data-autofocus="true"
          />
        </label>

        {editing && (
          <div className="field">
            <span>{t('recurring.applyAmount')}</span>
            <div className="segmented" role="group" aria-label={t('recurring.applyAmount')}>
              <button
                type="button"
                className={`segmented__option${amountScope === 'from' ? ' segmented__option--active' : ''}`}
                aria-pressed={amountScope === 'from'}
                onClick={() => setAmountScope('from')}
              >
                {t('recurring.fromMonth')}
              </button>
              <button
                type="button"
                className={`segmented__option${amountScope === 'all' ? ' segmented__option--active' : ''}`}
                aria-pressed={amountScope === 'all'}
                onClick={() => setAmountScope('all')}
              >
                {t('recurring.allMonths')}
              </button>
            </div>
            {amountScope === 'from' && (
              <DatePicker
                value={effectiveFrom}
                onChange={setEffectiveFrom}
                ariaLabel={t('recurring.effectiveFrom')}
              />
            )}
          </div>
        )}

        <div className="field">
          <span>{t('recurring.startDate')}</span>
          <DatePicker value={startDate} onChange={setStartDate} ariaLabel={t('recurring.startDate')} />
        </div>

        <div className="field-row">
          <label className="field field--narrow">
            <span>{t('recurring.every')}</span>
            <input
              inputMode="numeric"
              value={interval}
              onChange={(e) => setInterval(e.currentTarget.value)}
              required
            />
          </label>
          <div className="field field--grow">
            <span>{t('recurring.frequency')}</span>
            <Select
              value={unit}
              options={unitOptions}
              onChange={(v) => setUnit(v as FreqUnit)}
              ariaLabel={t('recurring.frequency')}
            />
          </div>
        </div>

        <div className="field">
          <span>{t('recurring.end')}</span>
          <div className="segmented" role="group" aria-label={t('recurring.end')}>
            {(['never', 'until'] as const).map((k) => (
              <button
                key={k}
                type="button"
                className={`segmented__option${endKind === k ? ' segmented__option--active' : ''}`}
                aria-pressed={endKind === k}
                onClick={() => setEndKind(k)}
              >
                {k === 'never' ? t('recurring.endNever') : t('recurring.endUntil')}
              </button>
            ))}
          </div>
          {endKind === 'until' && (
            <DatePicker value={endDate} onChange={setEndDate} ariaLabel={t('recurring.endDate')} />
          )}
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

        {errorMessage && <p className="field-error">{errorMessage}</p>}

        <div className="modal__actions">
          <span className="modal__spacer" />
          <button type="button" className="btn btn--ghost" onClick={onClose} disabled={busy}>
            {t('common.cancel')}
          </button>
          <button type="submit" className="btn" disabled={busy}>
            {editing ? t('recurring.save') : t('recurring.create')}
          </button>
        </div>
      </form>
    </Modal>
  );
}

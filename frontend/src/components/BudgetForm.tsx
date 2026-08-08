import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { AccountId, Budget, Month, RecurringRule } from '../api/types';
import {
  useCategories,
  useCreateBudget,
  useDeleteRule,
  useUpdateBudget,
} from '../api/hooks';
import { categoryIconText } from '../lib/categories';
import { isoDate, isoMonthOf } from '../lib/date';
import { formatMoney, isMoneyInput, normalizeAmountInput } from '../lib/money';
import { monthLabel } from '../lib/month';
import { DatePicker } from './DatePicker';
import { Modal } from './Modal';
import { Select } from './Select';

interface BudgetFormProps {
  accountId: AccountId;
  currency: string;
  /** The month the budget starts in when creating (the screen's selected month). */
  startMonth: Month;
  /** The budget being edited, or `null` to create a new one. */
  editing: Budget | null;
  /** Category ids already budgeted on this account, so a duplicate is unpickable. */
  takenCategoryIds: readonly number[];
  /** The account's expense rules, to spot one that already books a category. */
  rules: readonly RecurringRule[];
  onClose: () => void;
}

/** The first day of `month`, so the end-month picker can reuse `DatePicker`. */
const firstDayOf = (month: Month): string => isoDate(month.year, month.month, 1);

const isBefore = (a: Month, b: Month): boolean =>
  a.year < b.year || (a.year === b.year && a.month < b.month);

export function BudgetForm({
  accountId,
  currency,
  startMonth,
  editing,
  takenCategoryIds,
  rules,
  onClose,
}: BudgetFormProps) {
  const { t } = useTranslation();
  const create = useCreateBudget(accountId);
  const update = useUpdateBudget(accountId);
  const deleteRule = useDeleteRule(accountId);
  const { data: categories } = useCategories();

  const start = editing?.start ?? startMonth;
  const [categoryId, setCategoryId] = useState(
    editing != null ? String(editing.category_id) : '',
  );
  const [amount, setAmount] = useState(editing?.amount ?? '');
  // Tracked so a pre-existing rule's amount only seeds an untouched field.
  const [amountTouched, setAmountTouched] = useState(editing != null);
  const [endKind, setEndKind] = useState<'never' | 'until'>(
    editing?.end != null ? 'until' : 'never',
  );
  // Default the end picker to the month being viewed, not the budget's start:
  // for a long-running budget the start is in the past, and proposing it would
  // silently retire the budget for every month since. Never before the start.
  const [endDate, setEndDate] = useState(
    firstDayOf(editing?.end ?? (isBefore(startMonth, start) ? start : startMonth)),
  );
  const [dropRule, setDropRule] = useState(true);
  const [localError, setLocalError] = useState<string | null>(null);

  // A recurring expense in the same category books its full amount every month,
  // so it would consume the whole budget on day one and leave it inert (while
  // still double-counting against `available`). Offer to retire it.
  const selected = categoryId === '' ? null : Number(categoryId);
  const clashingRule =
    selected == null
      ? undefined
      : rules.find((rule) => rule.kind === 'expense' && rule.category_id === selected);

  function pickCategory(value: string) {
    setCategoryId(value);
    if (amountTouched) {
      return;
    }
    // Seed from the rule this budget is about to replace — a plain suggestion the
    // user can overwrite. The latest segment is what the rule charges today.
    // Clearing when there is no rule matters: otherwise switching from Rent to
    // Groceries would leave Rent's amount behind as if the user had typed it.
    const id = value === '' ? null : Number(value);
    const rule =
      id == null
        ? undefined
        : rules.find((r) => r.kind === 'expense' && r.category_id === id);
    setAmount(rule ? rule.amounts[rule.amounts.length - 1].amount : '');
  }

  const categoryOptions = (categories ?? [])
    .filter((c) => c.id === editing?.category_id || !takenCategoryIds.includes(c.id))
    .map((c) => ({
      value: String(c.id),
      label: (
        <span>
          {categoryIconText(c.icon)} {c.label}
        </span>
      ),
    }));

  const mutationError = create.error ?? update.error ?? deleteRule.error;
  const errorMessage = localError ?? mutationError?.message ?? null;
  const busy = create.isPending || update.isPending || deleteRule.isPending;

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    if (selected == null) {
      setLocalError(t('budgets.pickCategory'));
      return;
    }
    const normalizedAmount = normalizeAmountInput(amount);
    if (!isMoneyInput(normalizedAmount) || Number(normalizedAmount) <= 0) {
      setLocalError(t('entry.invalidAmount'));
      return;
    }
    // The picker returns a full date; a budget only has month granularity.
    const end: Month | null = endKind === 'never' ? null : isoMonthOf(endDate);
    if (end && (end.year < start.year || (end.year === start.year && end.month < start.month))) {
      setLocalError(t('budgets.endBeforeStart'));
      return;
    }
    setLocalError(null);

    if (editing) {
      update.mutate(
        { ...editing, category_id: selected, amount: normalizedAmount, end },
        { onSuccess: onClose },
      );
      return;
    }
    create.mutate(
      { account_id: accountId, category_id: selected, amount: normalizedAmount, start, end },
      {
        onSuccess: () => {
          // Deliberately after the budget exists and deliberately not atomic: if
          // this fails, the safe state remains (the rule still shows on the
          // Recurring screen, and the Budgets screen flags the clash).
          if (clashingRule && dropRule) {
            deleteRule.mutate(clashingRule.id, { onSuccess: onClose, onError: onClose });
            return;
          }
          onClose();
        },
      },
    );
  }

  return (
    <Modal label={editing ? t('budgets.edit') : t('budgets.new')} onClose={onClose}>
      <form className="entry-form" onSubmit={handleSubmit}>
        <h2>{editing ? t('budgets.edit') : t('budgets.new')}</h2>

        <div className="field">
          <span>{t('entry.category')}</span>
          {categoryOptions.length === 0 ? (
            <p className="muted">{t('budgets.allCategoriesBudgeted')}</p>
          ) : (
            <Select
              value={categoryId}
              options={categoryOptions}
              onChange={pickCategory}
              ariaLabel={t('entry.category')}
            />
          )}
        </div>

        {clashingRule && !editing && (
          <div className="budget-warning">
            <p>
              {t('budgets.ruleConflict', {
                amount: formatMoney(
                  clashingRule.amounts[clashingRule.amounts.length - 1].amount,
                  currency,
                ),
              })}
            </p>
            <label className="budget-warning__toggle">
              <input
                type="checkbox"
                checked={dropRule}
                onChange={(e) => setDropRule(e.currentTarget.checked)}
              />
              <span>{t('budgets.ruleConflictDelete')}</span>
            </label>
          </div>
        )}

        <label className="field">
          <span>{t('budgets.amount', { currency })}</span>
          <input
            inputMode="decimal"
            value={amount}
            onChange={(e) => {
              setAmount(e.currentTarget.value);
              setAmountTouched(true);
            }}
            placeholder={t('entry.amountPlaceholder')}
            required
            data-autofocus="true"
          />
        </label>

        {/* Read-only, like an account's anchor month: moving a budget's start
            would retroactively rewrite past months' `free` figures. */}
        <div className="field">
          <span>{t('budgets.startsIn')}</span>
          <p className="muted">{monthLabel(start)}</p>
        </div>

        <div className="field">
          <span>{t('budgets.end')}</span>
          <div className="segmented" role="group" aria-label={t('budgets.end')}>
            {(['never', 'until'] as const).map((k) => (
              <button
                key={k}
                type="button"
                className={`segmented__option${endKind === k ? ' segmented__option--active' : ''}`}
                aria-pressed={endKind === k}
                onClick={() => setEndKind(k)}
              >
                {k === 'never' ? t('budgets.endNever') : t('budgets.endUntil')}
              </button>
            ))}
          </div>
          {endKind === 'until' && (
            <>
              <DatePicker
                value={endDate}
                onChange={setEndDate}
                ariaLabel={t('budgets.endMonth')}
              />
              <p className="muted">{t('budgets.endMonthHint')}</p>
            </>
          )}
        </div>

        {errorMessage && <p className="field-error">{errorMessage}</p>}

        <div className="modal__actions">
          <span className="modal__spacer" />
          <button type="button" className="btn btn--ghost" onClick={onClose} disabled={busy}>
            {t('common.cancel')}
          </button>
          <button type="submit" className="btn" disabled={busy}>
            {editing ? t('budgets.save') : t('budgets.create')}
          </button>
        </div>
      </form>
    </Modal>
  );
}

import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { AccountId, Budget, BudgetSegment, Month, RecurringRule } from '../api/types';
import {
  useCategories,
  useCreateBudget,
  useDeleteRule,
  useUpdateBudget,
} from '../api/hooks';
import {
  amountInMonth,
  isBeforeMonth,
  isSameMonth,
  reanchorAmounts,
  withAmountFrom,
} from '../lib/budget';
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

/** How an edited amount applies: to every month, or from one month onward. */
type AmountScope = 'all' | 'from';

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
  const create = useCreateBudget();
  const update = useUpdateBudget();
  const deleteRule = useDeleteRule(accountId);
  const { data: categories, isPending: categoriesPending } = useCategories();

  // Editable: a budget's start is the answer to "from when does this apply?",
  // and with effective-dated amounts moving it earlier only extends the base
  // over months that had none. Without this there is no way to bring an existing
  // budget back to a month you paged to — and since a category can hold only one
  // budget, no way to cover that month at all.
  const [startDate, setStartDate] = useState(firstDayOf(editing?.start ?? startMonth));
  const start = isoMonthOf(startDate);
  const [categoryId, setCategoryId] = useState(
    editing != null ? String(editing.category_id) : '',
  );
  // The figure the *viewed* month actually reserves, not the newest one: editing
  // while looking at May must show May's amount, or "unchanged" would compare
  // against a later raise and silently discard the edit.
  const viewedAmount = editing ? amountInMonth(editing.amounts, startMonth) : '';
  const [amount, setAmount] = useState(viewedAmount);
  // Tracked so a pre-existing rule's amount only seeds an untouched field.
  const [amountTouched, setAmountTouched] = useState(editing != null);
  // Amount-change scope (edit only): default to "from this month" so a raise
  // never rewrites what earlier months were planned at.
  const [amountScope, setAmountScope] = useState<AmountScope>('from');
  const [endKind, setEndKind] = useState<'never' | 'until'>(
    editing?.end != null ? 'until' : 'never',
  );
  // Default the end picker to the month being viewed, not the budget's start:
  // for a long-running budget the start is in the past, and proposing it would
  // silently retire the budget for every month since. Never before the start.
  const [endDate, setEndDate] = useState(
    firstDayOf(editing?.end ?? (isBeforeMonth(startMonth, start) ? start : startMonth)),
  );
  // Starts off: the category may be the list's default rather than a deliberate
  // choice, and deleting a recurring rule the user never aimed at is destructive.
  // `pickCategory` turns it on, so an explicit pick still gets the helpful
  // default (a budget usually *does* replace the rule it's modelled on).
  const [dropRule, setDropRule] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  const someCategoryHidden = (categories ?? []).some(
    (c) => c.id !== editing?.category_id && takenCategoryIds.includes(c.id),
  );
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

  // A budget always needs a category, so there is no "none" option — which means
  // `Select` falls back to displaying the first one while `categoryId` is still
  // empty. Fall back to the same option here so the form acts on what it shows,
  // rather than rejecting a category the user can plainly see selected. Derived
  // rather than seeded into state: the list arrives asynchronously.
  const selectedCategoryId = categoryId === '' ? (categoryOptions[0]?.value ?? '') : categoryId;
  const selected = selectedCategoryId === '' ? null : Number(selectedCategoryId);

  // A recurring expense in the same category books its full amount every month,
  // so it would consume the whole budget on day one and leave it inert (while
  // still double-counting against `available`). Offer to retire it.
  const clashingRule =
    selected == null
      ? undefined
      : rules.find((rule) => rule.kind === 'expense' && rule.category_id === selected);

  function pickCategory(value: string) {
    setCategoryId(value);
    setDropRule(true);
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

  const mutationError = create.error ?? update.error ?? deleteRule.error;
  const errorMessage = localError ?? mutationError?.message ?? null;
  const busy = create.isPending || update.isPending || deleteRule.isPending;
  const hasHistory = editing != null && editing.amounts.length > 1;

  /**
   * The month an edit takes effect from: the one being viewed, clamped into the
   * budget's span so the base stays anchored at `start` and no breakpoint lands
   * after `end`, where it could never apply.
   */
  function effectiveMonth(end: Month | null): Month {
    if (isBeforeMonth(startMonth, start)) {
      return start;
    }
    if (end && isBeforeMonth(end, startMonth)) {
      return end;
    }
    return startMonth;
  }

  /**
   * The amount history to persist on an edit. The start month is applied first
   * (it decides where the base sits), then the amount change on top:
   *   - "all months" → collapse to a single base segment at the new amount;
   *   - "from"       → set the viewed month's segment, leaving every other
   *                    breakpoint (earlier *and* later) alone.
   */
  function nextAmounts(budget: Budget, trimmed: string, end: Month | null): BudgetSegment[] {
    const rebased = isSameMonth(budget.start, start)
      ? budget.amounts
      : reanchorAmounts(budget.amounts, start);

    // Checked before the unchanged shortcut below: flattening is a structural
    // change even when the figure itself is the same, and it is the only way to
    // clear a history — including back under the segment cap.
    if (amountScope === 'all') {
      return [{ effective_from: start, amount: trimmed }];
    }
    // Numeric compare (not string) so "400" vs the stored "400.00" reads as
    // unchanged; a control decision only, never a stored money value, and
    // amounts are bounded well within float precision by the core's cap.
    if (Number(trimmed) === Number(viewedAmount)) {
      return rebased;
    }
    return withAmountFrom(rebased, effectiveMonth(end), trimmed);
  }

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
        {
          ...editing,
          category_id: selected,
          amounts: nextAmounts(editing, normalizedAmount, end),
          start,
          end,
        },
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
          {/* Distinguish "still loading" from "genuinely none left": an empty
              list mid-fetch would otherwise claim every category is budgeted. */}
          {categoriesPending ? (
            <p className="muted">{t('common.loading')}</p>
          ) : categoryOptions.length === 0 ? (
            <p className="muted">{t('budgets.allCategoriesBudgeted')}</p>
          ) : (
            <Select
              value={selectedCategoryId}
              options={categoryOptions}
              onChange={pickCategory}
              ariaLabel={t('entry.category')}
            />
          )}
          {/* A category holds at most one budget, so an already-budgeted one is
              missing from the list even when its budget doesn't cover the month
              being viewed. Say so, and point at the fix (moving that budget's
              start), rather than leaving a silent gap. */}
          {!editing && !categoriesPending && someCategoryHidden && (
            <p className="muted">{t('budgets.categoryTaken')}</p>
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

        {hasHistory && (
          <div className="rule-history">
            <span className="muted">{t('budgets.amountHistory')}</span>
            <ul>
              {editing.amounts.map((s) => (
                <li key={`${s.effective_from.year}-${s.effective_from.month}`}>
                  {formatMoney(s.amount, currency)} ·{' '}
                  {t('budgets.startsFrom', { month: monthLabel(s.effective_from) })}
                </li>
              ))}
            </ul>
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

        {/* Offered once the two scopes can actually differ — i.e. the budget has
            an earlier month to leave alone, or later breakpoints that "all
            months" would flatten. With a single amount edited at its start month
            both do the same thing, so the choice would be noise. */}
        {editing && (isBeforeMonth(start, startMonth) || hasHistory) && (
          <div className="field">
            <span>{t('budgets.applyAmount')}</span>
            <div className="segmented" role="group" aria-label={t('budgets.applyAmount')}>
              <button
                type="button"
                className={`segmented__option${amountScope === 'from' ? ' segmented__option--active' : ''}`}
                aria-pressed={amountScope === 'from'}
                onClick={() => setAmountScope('from')}
              >
                {t('budgets.fromMonth', { month: monthLabel(startMonth) })}
              </button>
              <button
                type="button"
                className={`segmented__option${amountScope === 'all' ? ' segmented__option--active' : ''}`}
                aria-pressed={amountScope === 'all'}
                onClick={() => setAmountScope('all')}
              >
                {t('budgets.allMonths')}
              </button>
            </div>
          </div>
        )}

        <div className="field">
          <span>{t('budgets.startsIn')}</span>
          <DatePicker
            value={startDate}
            onChange={setStartDate}
            ariaLabel={t('budgets.startsIn')}
          />
          <p className="muted">{t('budgets.monthHint')}</p>
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
              <p className="muted">{t('budgets.monthHint')}</p>
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

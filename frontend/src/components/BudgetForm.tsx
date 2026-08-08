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
  clashingExpenseRule,
  isBeforeMonth,
  isSameMonth,
  reanchorAmounts,
  withAmountForMonth,
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

/**
 * How far an edited amount reaches. Defaults to the narrowest, `month`: every
 * complaint about this form has been a change spreading further than intended,
 * and the other two are one tap away with their effect spelled out.
 */
type AmountScope = 'month' | 'from' | 'all';

/** `view` clamped into `[start, end]` — the month an edit there really lands on. */
function clampToSpan(view: Month, start: Month, end?: Month | null): Month {
  if (isBeforeMonth(view, start)) {
    return start;
  }
  if (end && isBeforeMonth(end, view)) {
    return end;
  }
  return view;
}

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
  //
  // Opening one from a month it doesn't reach yet proposes *that* month, because
  // that is why you opened it. The field shows the proposal plainly rather than
  // explaining that you have to go and change it.
  const [startDate, setStartDate] = useState(
    firstDayOf(
      editing && isBeforeMonth(startMonth, editing.start) ? startMonth : (editing?.start ?? startMonth),
    ),
  );
  const [categoryId, setCategoryId] = useState(
    editing != null ? String(editing.category_id) : '',
  );
  // `null` while the field is untouched, so it can follow the start month below
  // instead of freezing at whatever it showed when the form opened.
  const [amountInput, setAmountInput] = useState<string | null>(null);
  const [amountScope, setAmountScope] = useState<AmountScope>('month');
  const [endKind, setEndKind] = useState<'never' | 'until'>(
    editing?.end != null ? 'until' : 'never',
  );
  // The stored end is kept as-is, deliberately *not* mirroring the start's
  // proposal above. Extending a start backwards covers months that had no budget
  // at all; un-ending a budget someone deliberately ended is a different thing,
  // and doing it as a side effect of opening the form to fix an amount would be
  // a genuine surprise. A budget with no end defaults to the viewed month, which
  // the start proposal guarantees is never before the start.
  const [endDate, setEndDate] = useState(firstDayOf(editing?.end ?? startMonth));
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

  // The pickers return full dates; a budget only has month granularity. All
  // derived, so every figure below follows the *edited* span rather than the
  // stored one — the two disagreeing is how a start moved past a breakpoint used
  // to save an amount the field never showed.
  const start = isoMonthOf(startDate);
  const end: Month | null = endKind === 'never' ? null : isoMonthOf(endDate);
  const effectiveMonth: Month = clampToSpan(startMonth, start, end);
  /** The stored history re-anchored to the edited start, if it moved. */
  const rebasedAmounts =
    editing && !isSameMonth(editing.start, start)
      ? reanchorAmounts(editing.amounts, start)
      : (editing?.amounts ?? []);
  /** What the month being edited runs at right now — the field's untouched value
   *  and the baseline "unchanged" compares against. */
  const currentAmount = editing ? amountInMonth(rebasedAmounts, effectiveMonth) : '';
  const amount = amountInput ?? currentAmount;

  // A recurring expense overlapping this budget's span would consume it on day
  // one while still double-counting against `available`. Offer to retire it.
  const clashingRule =
    selected == null ? undefined : clashingExpenseRule(rules, selected, { start, end });

  function pickCategory(value: string) {
    setCategoryId(value);
    setDropRule(true);
    if (amountInput !== null) {
      return;
    }
    // Seed from the rule this budget is about to replace — a plain suggestion the
    // user can overwrite. The latest segment is what the rule charges today.
    // Clearing when there is no rule matters: otherwise switching from Rent to
    // Groceries would leave Rent's amount behind as if the user had typed it.
    const id = value === '' ? null : Number(value);
    const rule = id == null ? undefined : clashingExpenseRule(rules, id, { start, end });
    setAmountInput(rule ? rule.amounts[rule.amounts.length - 1].amount : '');
  }

  const mutationError = create.error ?? update.error ?? deleteRule.error;
  const errorMessage = localError ?? mutationError?.message ?? null;
  const busy = create.isPending || update.isPending || deleteRule.isPending;
  const hasHistory = editing != null && editing.amounts.length > 1;

  /**
   * The amount history to persist on an edit. `rebasedAmounts` has already moved
   * the base to the edited start; this layers the amount change on top:
   *   - "all months" → collapse to a single base segment at the new amount;
   *   - "from"       → set that month's segment, leaving every other breakpoint
   *                    (earlier *and* later) alone;
   *   - "month"      → that plus a breakpoint after it restoring the old figure.
   */
  function nextAmounts(trimmed: string): BudgetSegment[] {
    // Checked before the unchanged shortcut below: flattening is a structural
    // change even when the figure itself is the same, and it is the only way to
    // clear a history — including back under the segment cap.
    if (amountScope === 'all') {
      return [{ effective_from: start, amount: trimmed }];
    }
    // Numeric compare (not string) so "400" vs the stored "400.00" reads as
    // unchanged; a control decision only, never a stored money value, and
    // amounts are bounded well within float precision by the core's cap.
    if (Number(trimmed) === Number(currentAmount)) {
      return rebasedAmounts;
    }
    return amountScope === 'month'
      ? withAmountForMonth(rebasedAmounts, effectiveMonth, trimmed, end)
      : withAmountFrom(rebasedAmounts, effectiveMonth, trimmed);
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
    if (end && isBeforeMonth(end, start)) {
      setLocalError(t('budgets.endBeforeStart'));
      return;
    }
    setLocalError(null);

    if (editing) {
      update.mutate(
        {
          ...editing,
          category_id: selected,
          amounts: nextAmounts(normalizedAmount),
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
            onChange={(e) => setAmountInput(e.currentTarget.value)}
            placeholder={t('entry.amountPlaceholder')}
            required
            data-autofocus="true"
          />
        </label>

        {/* Always shown when editing, never conditionally hidden: how far a
            change reaches is the thing about this form people get wrong, and the
            selected option spells out its own effect below. */}
        {editing && (
          <div className="field">
            <span>{t('budgets.applyAmount')}</span>
            <div className="segmented" role="group" aria-label={t('budgets.applyAmount')}>
              {(['month', 'from', 'all'] as const).map((scope) => (
                <button
                  key={scope}
                  type="button"
                  className={`segmented__option${amountScope === scope ? ' segmented__option--active' : ''}`}
                  aria-pressed={amountScope === scope}
                  onClick={() => setAmountScope(scope)}
                >
                  {scope === 'all'
                    ? t('budgets.allMonths')
                    : t(scope === 'month' ? 'budgets.thisMonthOnly' : 'budgets.fromMonth', {
                        month: monthLabel(effectiveMonth),
                      })}
                </button>
              ))}
            </div>
            <p className="muted">
              {t(`budgets.scopeHint.${amountScope}`, { month: monthLabel(effectiveMonth) })}
            </p>
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

import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { Account, Budget, BudgetStatus, Category, CategoryId, Month } from '../api/types';
import {
  useBudgets,
  useCategories,
  useDeleteBudget,
  useMonthBudgets,
  useRules,
} from '../api/hooks';
import { BudgetForm } from '../components/BudgetForm';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { ErrorBanner } from '../components/ErrorBanner';
import { MonthNav } from '../components/MonthNav';
import { Spinner } from '../components/Spinner';
import { accountColor } from '../lib/accountColor';
import { amountInMonth, isBeforeMonth } from '../lib/budget';
import { categoryIconText } from '../lib/categories';
import { formatMoney, parseMoneyForDisplay } from '../lib/money';
import { monthLabel } from '../lib/month';
import { useSwipe } from '../lib/swipe';
import { useSelectedMonth } from '../state/contexts';

type Dialog =
  | { type: 'none' }
  | { type: 'create' }
  | { type: 'edit'; budget: Budget }
  | { type: 'delete'; budget: Budget; label: string };

/**
 * The budgets screen: for the selected month, each budget's planned amount and
 * what has actually been spent in its category. A budget holds its amount back
 * from the month's free-to-spend figure; spending consumes that reservation
 * rather than stacking on top of it (see `docs/DESIGN.md` §12).
 */
export function BudgetsScreen({ account }: { account: Account }) {
  const { t } = useTranslation();
  const { month, next, prev } = useSelectedMonth();
  const statuses = useMonthBudgets(account.id, month);
  const budgets = useBudgets(account.id);
  const rules = useRules(account.id);
  const { data: categories } = useCategories();
  const del = useDeleteBudget();
  const [dialog, setDialog] = useState<Dialog>({ type: 'none' });
  const close = () => setDialog({ type: 'none' });

  // Swipe paging mirrors the month and stats screens (left → next, right → prev).
  const swipe = useSwipe({ onSwipeLeft: next, onSwipeRight: prev });

  const categoryById = useMemo(() => {
    const map = new Map<CategoryId, Category>();
    for (const category of categories ?? []) {
      map.set(category.id, category);
    }
    return map;
  }, [categories]);

  const budgetById = useMemo(() => {
    const map = new Map<number, Budget>();
    for (const budget of budgets.data ?? []) {
      map.set(budget.id, budget);
    }
    return map;
  }, [budgets.data]);

  // A summary account aggregates its members' budgets; it owns none itself, so it
  // is read-only here just as it is for entries and rules.
  const isSummary = account.kind === 'summary';

  function label(categoryId: CategoryId): string {
    return categoryById.get(categoryId)?.label ?? t('stats.other');
  }

  function icon(categoryId: CategoryId): string {
    const category = categoryById.get(categoryId);
    return category ? categoryIconText(category.icon) : '🏷️';
  }

  /**
   * An expense rule that also books this category, double-counting against it.
   *
   * Only meaningful on a normal account: `list_rules` returns a summary's *own*
   * rules, and a summary owns none. The clash — and the fix for it — lives on the
   * member account, whose own Budgets screen flags it.
   */
  function clashingRuleFor(categoryId: CategoryId) {
    if (isSummary) {
      return undefined;
    }
    return (rules.data ?? []).find(
      (rule) => rule.kind === 'expense' && rule.category_id === categoryId,
    );
  }

  /** Budgets that exist but don't apply to the selected month, so they stay
   *  reachable for editing instead of vanishing when you page months. */
  const inactive = (budgets.data ?? []).filter(
    (budget) => !statuses.data?.some((status) => status.budget_id === budget.id),
  );

  /**
   * Why a budget isn't running in the selected month, and **the month that
   * answer names** — the amount shown beside it has to be the one in effect
   * then, not the newest segment, or a budget of 100 from August raised to 130
   * in November reads "from August: 130" when viewed from July.
   *
   * Branches on where the selected month sits rather than on whether an end
   * exists: a budget that hasn't started yet would otherwise read as already
   * finished. A budget can also be inactive for a third reason (the month
   * predates the account's anchor, so the ledger doesn't cover it); that falls
   * through to its start month, which is true if not the full story.
   */
  function inactiveIn(budget: Budget): { label: string; at: Month } {
    if (budget.end && isBeforeMonth(budget.end, month)) {
      return {
        label: t('budgets.ranUntil', { month: monthLabel(budget.end) }),
        at: budget.end,
      };
    }
    return {
      label: t('budgets.startsFrom', { month: monthLabel(budget.start) }),
      at: budget.start,
    };
  }

  if (statuses.isPending || budgets.isPending) {
    return <Spinner />;
  }
  // Checked separately (rather than via a combined `??`) so TypeScript narrows
  // each query's `data` to the loaded shape below.
  if (statuses.error) {
    return <ErrorBanner error={statuses.error} />;
  }
  if (budgets.error) {
    return <ErrorBanner error={budgets.error} />;
  }

  return (
    <section className="screen budgets-screen" {...swipe}>
      <div className="screen__header">
        <h2>{t('budgets.title')}</h2>
        {!isSummary && (
          <button type="button" className="btn" onClick={() => setDialog({ type: 'create' })}>
            {t('budgets.add')}
          </button>
        )}
      </div>
      <MonthNav />

      {statuses.data.length === 0 ? (
        <p className="muted">{isSummary ? t('budgets.emptySummary') : t('budgets.empty')}</p>
      ) : (
        <ul className="stats-list">
          {statuses.data.map((status) => (
            <BudgetRow
              key={status.budget_id}
              status={status}
              currency={account.currency}
              icon={icon(status.category_id)}
              label={label(status.category_id)}
              overLabel={t('budgets.over')}
              ruleWarning={
                clashingRuleFor(status.category_id) ? t('budgets.ruleConflictRow') : null
              }
              tagColor={isSummary ? accountColor(status.account_id) : null}
              onEdit={
                isSummary
                  ? null
                  : () => {
                      const budget = budgetById.get(status.budget_id);
                      if (budget) {
                        setDialog({ type: 'edit', budget });
                      }
                    }
              }
              onDelete={
                isSummary
                  ? null
                  : () => {
                      const budget = budgetById.get(status.budget_id);
                      if (budget) {
                        setDialog({
                          type: 'delete',
                          budget,
                          label: label(status.category_id),
                        });
                      }
                    }
              }
              deleteAria={t('budgets.deleteAria', { name: label(status.category_id) })}
            />
          ))}
        </ul>
      )}

      {!isSummary && inactive.length > 0 && (
        <>
          <p className="muted budgets-screen__section">{t('budgets.notThisMonth')}</p>
          <ul className="account-list">
            {inactive.map((budget) => {
              const { label: reason, at } = inactiveIn(budget);
              return (
                <li key={budget.id} className="account-list__row">
                  <button
                    type="button"
                    className="account-list__main rule-row"
                    onClick={() => setDialog({ type: 'edit', budget })}
                  >
                    <span className="account-list__icon">{icon(budget.category_id)}</span>
                    <span className="rule-row__main">
                      <span className="account-list__name">{label(budget.category_id)}</span>
                      <span className="rule-row__meta">{reason}</span>
                    </span>
                    <span className="amount">
                      {formatMoney(amountInMonth(budget.amounts, at), account.currency)}
                    </span>
                  </button>
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label={t('budgets.deleteAria', { name: label(budget.category_id) })}
                    onClick={() =>
                      setDialog({ type: 'delete', budget, label: label(budget.category_id) })
                    }
                  >
                    ✕
                  </button>
                </li>
              );
            })}
          </ul>
        </>
      )}

      {(dialog.type === 'create' || dialog.type === 'edit') && (
        <BudgetForm
          accountId={account.id}
          currency={account.currency}
          startMonth={month}
          editing={dialog.type === 'edit' ? dialog.budget : null}
          takenCategoryIds={(budgets.data ?? []).map((b) => b.category_id)}
          rules={rules.data ?? []}
          onClose={close}
        />
      )}
      {dialog.type === 'delete' && (
        <ConfirmDialog
          title={t('budgets.deleteTitle')}
          message={t('budgets.deleteWarning', { name: dialog.label })}
          confirmLabel={t('budgets.deleteConfirm')}
          busy={del.isPending}
          onCancel={close}
          onConfirm={() => del.mutate(dialog.budget.id, { onSuccess: close })}
        />
      )}
    </section>
  );
}

interface BudgetRowProps {
  status: BudgetStatus;
  currency: string;
  icon: string;
  label: string;
  overLabel: string;
  /** Set when a recurring expense also books this category (see BudgetForm). */
  ruleWarning: string | null;
  /** Source-account accent for a summary account's combined list, else null. */
  tagColor: string | null;
  onEdit: (() => void) | null;
  onDelete: (() => void) | null;
  deleteAria: string;
}

function BudgetRow({
  status,
  currency,
  icon,
  label,
  overLabel,
  ruleWarning,
  tagColor,
  onEdit,
  onDelete,
  deleteAria,
}: BudgetRowProps) {
  // Display-only ratio, exactly like the stats bars: the authoritative figures
  // stay the money strings above.
  const planned = parseMoneyForDisplay(status.planned);
  const actual = parseMoneyForDisplay(status.actual);
  const percent = planned > 0 ? (actual / planned) * 100 : 0;
  const over = actual > planned;

  const head = (
    <div className="stats-row__head">
      {tagColor && (
        <span className="budget-row__tag" style={{ backgroundColor: tagColor }} aria-hidden="true" />
      )}
      <span className="stats-row__icon" aria-hidden="true">
        {icon}
      </span>
      <span className="stats-row__label">{label}</span>
      <span className={`amount${over ? ' amount--expense' : ''}`}>
        {formatMoney(status.actual, currency)} / {formatMoney(status.planned, currency)}
      </span>
    </div>
  );

  return (
    <li className="stats-row budget-row">
      {/* The delete button is a flex sibling, not an overlay, so it can never
          cover the amounts however long the label or figures get. */}
      <div className="budget-row__top">
        {onEdit ? (
          <button type="button" className="budget-row__main" onClick={onEdit}>
            {head}
          </button>
        ) : (
          head
        )}
        {onDelete && (
          <button
            type="button"
            className="icon-btn budget-row__delete"
            aria-label={deleteAria}
            onClick={onDelete}
          >
            ✕
          </button>
        )}
      </div>
      <div className="stats-row__track">
        <div
          className={`stats-row__fill${over ? ' stats-row__fill--over' : ''}`}
          style={{ width: `${Math.min(percent, 100)}%` }}
        />
        <span className="stats-row__pct">
          {Math.round(percent)}%{over ? ` · ${overLabel}` : ''}
        </span>
      </div>
      {ruleWarning && <p className="budget-row__warning">{ruleWarning}</p>}
    </li>
  );
}

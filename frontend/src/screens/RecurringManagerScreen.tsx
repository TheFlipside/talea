import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { Account, Category, RecurringRule } from '../api/types';
import { useCategories, useDeleteRule, useRules } from '../api/hooks';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { ErrorBanner } from '../components/ErrorBanner';
import { RuleForm } from '../components/RuleForm';
import { Spinner } from '../components/Spinner';
import { categoryIconText } from '../lib/categories';
import { formatMoney } from '../lib/money';

type Dialog =
  | { type: 'none' }
  | { type: 'create' }
  | { type: 'edit'; rule: RecurringRule }
  | { type: 'delete'; rule: RecurringRule };

export function RecurringManagerScreen({ account }: { account: Account }) {
  const { t } = useTranslation();
  const rules = useRules(account.id);
  const { data: categories } = useCategories();
  const del = useDeleteRule(account.id);
  const [dialog, setDialog] = useState<Dialog>({ type: 'none' });
  const close = () => setDialog({ type: 'none' });

  if (rules.isPending) {
    return <Spinner />;
  }
  if (rules.error) {
    return <ErrorBanner error={rules.error} />;
  }

  // A summary account has no rules of its own; recurring rules are managed on the
  // member accounts.
  const isSummary = account.kind === 'summary';

  const byId = new Map<number, Category>((categories ?? []).map((c) => [c.id, c]));
  const cadence = (rule: RecurringRule) =>
    t(`recurring.cadence.${rule.frequency.unit}`, { count: rule.frequency.interval });

  function title(rule: RecurringRule): string {
    if (rule.note) {
      return rule.note;
    }
    const category = rule.category_id != null ? byId.get(rule.category_id) : undefined;
    return category?.label ?? t(rule.kind === 'income' ? 'entry.income' : 'entry.expense');
  }

  function icon(rule: RecurringRule): string {
    const category = rule.category_id != null ? byId.get(rule.category_id) : undefined;
    if (category) {
      return categoryIconText(category.icon);
    }
    return rule.kind === 'income' ? '＋' : '－';
  }

  return (
    <section className="screen recurring-screen">
      <div className="screen__header">
        <h2>{t('recurring.title')}</h2>
        {!isSummary && (
          <button type="button" className="btn" onClick={() => setDialog({ type: 'create' })}>
            {t('recurring.add')}
          </button>
        )}
      </div>

      {isSummary ? (
        <p className="muted">{t('summary.combinedHint')}</p>
      ) : rules.data.length === 0 ? (
        <p className="muted">{t('recurring.empty')}</p>
      ) : (
        <ul className="account-list">
          {rules.data.map((rule) => {
            const latest = rule.amounts[rule.amounts.length - 1].amount;
            const income = rule.kind === 'income';
            return (
              <li key={rule.id} className="account-list__row">
                <button
                  type="button"
                  className="account-list__main rule-row"
                  onClick={() => setDialog({ type: 'edit', rule })}
                >
                  <span className="account-list__icon">{icon(rule)}</span>
                  <span className="rule-row__main">
                    <span className="account-list__name">{title(rule)}</span>
                    <span className="rule-row__meta">{cadence(rule)}</span>
                  </span>
                  <span className={`amount ${income ? 'amount--income' : 'amount--expense'}`}>
                    {income ? '+' : '−'}
                    {formatMoney(latest, account.currency)}
                  </span>
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  aria-label={t('recurring.deleteAria', { name: title(rule) })}
                  onClick={() => setDialog({ type: 'delete', rule })}
                >
                  ✕
                </button>
              </li>
            );
          })}
        </ul>
      )}

      {dialog.type === 'create' && (
        <RuleForm accountId={account.id} currency={account.currency} editing={null} onClose={close} />
      )}
      {dialog.type === 'edit' && (
        <RuleForm
          accountId={account.id}
          currency={account.currency}
          editing={dialog.rule}
          onClose={close}
        />
      )}
      {dialog.type === 'delete' && (
        <ConfirmDialog
          title={t('recurring.deleteTitle')}
          message={t('recurring.deleteWarning', { name: title(dialog.rule) })}
          confirmLabel={t('recurring.deleteConfirm')}
          busy={del.isPending}
          onCancel={close}
          onConfirm={() => del.mutate(dialog.rule.id, { onSuccess: close })}
        />
      )}
    </section>
  );
}

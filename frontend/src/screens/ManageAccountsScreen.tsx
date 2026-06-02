import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { Account } from '../api/types';
import { useAccounts, useDeleteAccount } from '../api/hooks';
import { AccountForm } from '../components/AccountForm';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { ErrorBanner } from '../components/ErrorBanner';
import { Modal } from '../components/Modal';
import { Spinner } from '../components/Spinner';

type Dialog =
  | { type: 'none' }
  | { type: 'create' }
  | { type: 'edit'; account: Account }
  | { type: 'delete'; account: Account };

export function ManageAccountsScreen() {
  const { t } = useTranslation();
  const { data: accounts, isPending, error } = useAccounts();
  const del = useDeleteAccount();
  const [dialog, setDialog] = useState<Dialog>({ type: 'none' });
  const close = () => setDialog({ type: 'none' });

  if (isPending) {
    return <Spinner />;
  }
  if (error) {
    return <ErrorBanner error={error} />;
  }

  return (
    <section className="screen accounts-screen">
      <div className="screen__header">
        <h2>{t('accounts.title')}</h2>
        <button type="button" className="btn" onClick={() => setDialog({ type: 'create' })}>
          {t('accounts.add')}
        </button>
      </div>

      <ul className="account-list">
        {accounts.map((account) => (
          <li key={account.id} className="account-list__row">
            <button
              type="button"
              className="account-list__main"
              onClick={() => setDialog({ type: 'edit', account })}
            >
              <span className="account-list__icon">{account.icon}</span>
              <span className="account-list__name">{account.name}</span>
              <span className="muted">{account.currency}</span>
            </button>
            <button
              type="button"
              className="icon-btn"
              aria-label={t('accounts.deleteAria', { name: account.name })}
              disabled={accounts.length <= 1}
              onClick={() => setDialog({ type: 'delete', account })}
            >
              ✕
            </button>
          </li>
        ))}
      </ul>

      {dialog.type === 'create' && (
        <Modal label={t('account.new')} onClose={close}>
          <AccountForm mode="create" onDone={close} />
        </Modal>
      )}
      {dialog.type === 'edit' && (
        <Modal label={t('account.edit')} onClose={close}>
          <AccountForm mode="edit" account={dialog.account} onDone={close} />
        </Modal>
      )}
      {dialog.type === 'delete' && (
        <ConfirmDialog
          title={t('accounts.deleteTitle')}
          // Surface a backend refusal (e.g. the account still feeds a summary)
          // in place of the warning, leaving the dialog open.
          message={
            del.error
              ? del.error.message
              : t('accounts.deleteWarning', { name: dialog.account.name })
          }
          confirmLabel={t('accounts.deleteConfirm')}
          busy={del.isPending}
          onCancel={close}
          onConfirm={() => del.mutate(dialog.account.id, { onSuccess: close })}
        />
      )}
    </section>
  );
}

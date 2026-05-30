import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { Category } from '../api/types';
import { useCategories, useDeleteCategory } from '../api/hooks';
import { CategoryForm } from '../components/CategoryForm';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { ErrorBanner } from '../components/ErrorBanner';
import { Modal } from '../components/Modal';
import { Spinner } from '../components/Spinner';
import { categoryIconText } from '../lib/categories';

type Dialog =
  | { type: 'none' }
  | { type: 'create' }
  | { type: 'edit'; category: Category }
  | { type: 'delete'; category: Category };

export function CategoryManagerScreen() {
  const { t } = useTranslation();
  const { data: categories, isPending, error } = useCategories();
  const del = useDeleteCategory();
  const [dialog, setDialog] = useState<Dialog>({ type: 'none' });
  const close = () => setDialog({ type: 'none' });

  if (isPending) {
    return <Spinner />;
  }
  if (error) {
    return <ErrorBanner error={error} />;
  }

  return (
    <section className="screen categories-screen">
      <div className="screen__header">
        <h2>{t('categories.title')}</h2>
        <button type="button" className="btn" onClick={() => setDialog({ type: 'create' })}>
          {t('categories.add')}
        </button>
      </div>

      {categories.length === 0 ? (
        <p className="muted">{t('categories.empty')}</p>
      ) : (
        <ul className="account-list">
          {categories.map((category) => (
            <li key={category.id} className="account-list__row">
              <button
                type="button"
                className="account-list__main"
                onClick={() => setDialog({ type: 'edit', category })}
              >
                <span className="account-list__icon">{categoryIconText(category.icon)}</span>
                <span className="account-list__name">{category.label}</span>
              </button>
              <button
                type="button"
                className="icon-btn"
                aria-label={t('categories.deleteAria', { name: category.label })}
                onClick={() => setDialog({ type: 'delete', category })}
              >
                ✕
              </button>
            </li>
          ))}
        </ul>
      )}

      {dialog.type === 'create' && (
        <Modal label={t('category.new')} onClose={close}>
          <CategoryForm mode="create" onDone={close} />
        </Modal>
      )}
      {dialog.type === 'edit' && (
        <Modal label={t('category.edit')} onClose={close}>
          <CategoryForm mode="edit" category={dialog.category} onDone={close} />
        </Modal>
      )}
      {dialog.type === 'delete' && (
        <ConfirmDialog
          title={t('categories.deleteTitle')}
          message={t('categories.deleteWarning', { name: dialog.category.label })}
          confirmLabel={t('categories.deleteConfirm')}
          busy={del.isPending}
          onCancel={close}
          onConfirm={() => del.mutate(dialog.category.id, { onSuccess: close })}
        />
      )}
    </section>
  );
}

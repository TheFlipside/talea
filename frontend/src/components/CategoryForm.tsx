import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { Category, CategoryIcon } from '../api/types';
import { useCreateCategory, useUpdateCategory } from '../api/hooks';
import { EmojiPicker } from './EmojiPicker';

interface CategoryFormProps {
  mode: 'create' | 'edit';
  category?: Category;
  onDone: () => void;
}

export function CategoryForm({ mode, category, onDone }: CategoryFormProps) {
  const { t } = useTranslation();
  const create = useCreateCategory();
  const update = useUpdateCategory();

  const [label, setLabel] = useState(category?.label ?? '');
  // Track the whole icon so an existing preset icon is preserved unless the user
  // actually picks an emoji (the picker only edits emoji icons).
  const [icon, setIcon] = useState<CategoryIcon>(category?.icon ?? { emoji: '🏷️' });
  const emoji = 'emoji' in icon ? icon.emoji : '🏷️';
  const [localError, setLocalError] = useState<string | null>(null);

  const errorMessage = localError ?? create.error?.message ?? update.error?.message ?? null;
  const busy = create.isPending || update.isPending;

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const trimmed = label.trim();
    if (trimmed === '') {
      setLocalError(t('category.errorLabel'));
      return;
    }
    setLocalError(null);
    if (mode === 'edit' && category) {
      update.mutate({ id: category.id, label: trimmed, icon }, { onSuccess: onDone });
    } else {
      create.mutate({ label: trimmed, icon }, { onSuccess: onDone });
    }
  }

  return (
    <form className="card category-form" onSubmit={handleSubmit}>
      <h2>{mode === 'edit' ? t('category.edit') : t('category.new')}</h2>

      <div className="field-row">
        <div className="field field--narrow">
          <span>{t('category.icon')}</span>
          <EmojiPicker
            value={emoji}
            onChange={(picked) => setIcon({ emoji: picked })}
            ariaLabel={t('category.icon')}
          />
        </div>
        <label className="field field--grow">
          <span>{t('category.label')}</span>
          <input
            value={label}
            onChange={(e) => setLabel(e.currentTarget.value)}
            placeholder={t('category.labelPlaceholder')}
            required
            autoFocus
          />
        </label>
      </div>

      {errorMessage && <p className="field-error">{errorMessage}</p>}

      <div className="modal__actions">
        <span className="modal__spacer" />
        <button type="button" className="btn btn--ghost" onClick={onDone} disabled={busy}>
          {t('common.cancel')}
        </button>
        <button type="submit" className="btn" disabled={busy}>
          {mode === 'edit' ? t('category.save') : t('category.create')}
        </button>
      </div>
    </form>
  );
}

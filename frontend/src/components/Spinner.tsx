import { useTranslation } from 'react-i18next';

export function Spinner({ label }: { label?: string }) {
  const { t } = useTranslation();
  return (
    <div className="spinner" role="status" aria-live="polite">
      {label ?? t('common.loading')}
    </div>
  );
}

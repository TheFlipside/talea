import { useTranslation } from 'react-i18next';

import type { CommandError, CommandErrorCode } from '../api/types';

// Explicit map (not a runtime-composed key) so only known codes resolve a tone.
const TONE_KEYS: Record<CommandErrorCode, string> = {
  validation: 'errors.validation',
  not_found: 'errors.not_found',
  database: 'errors.database',
  corrupt: 'errors.corrupt',
};

export function ErrorBanner({ error }: { error: CommandError }) {
  const { t } = useTranslation();
  return (
    <div className="error-banner" role="alert">
      <strong>{t(TONE_KEYS[error.code])}</strong>
      <span>{error.message}</span>
    </div>
  );
}

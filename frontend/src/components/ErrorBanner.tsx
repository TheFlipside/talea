import type { CommandError } from '../api/types';

const TONE: Record<CommandError['code'], string> = {
  validation: 'Please check your input.',
  not_found: 'That item no longer exists.',
  database: 'Something went wrong reading your data.',
  corrupt: 'Your data file appears to be corrupt.',
};

export function ErrorBanner({ error }: { error: CommandError }) {
  return (
    <div className="error-banner" role="alert">
      <strong>{TONE[error.code]}</strong>
      <span>{error.message}</span>
    </div>
  );
}

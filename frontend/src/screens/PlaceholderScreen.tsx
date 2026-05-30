import { useTranslation } from 'react-i18next';

type PlaceholderKey = 'placeholder.recurring';

/** A "coming soon" placeholder for screens not yet built. */
export function PlaceholderScreen({ titleKey }: { titleKey: PlaceholderKey }) {
  const { t } = useTranslation();
  return (
    <section className="screen placeholder-screen">
      <h2>{t(titleKey)}</h2>
      <p className="muted">{t('placeholder.body')}</p>
    </section>
  );
}

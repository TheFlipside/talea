import type { ReactElement } from 'react';
import { useTranslation } from 'react-i18next';

import { useNavigation, type Screen } from '../state/contexts';
import { AccountsIcon, CategoriesIcon, RecurringIcon, StatsIcon } from './icons';

const ITEMS: { screen: Screen; labelKey: string; Icon: () => ReactElement }[] = [
  { screen: 'accounts', labelKey: 'nav.accounts', Icon: AccountsIcon },
  { screen: 'categories', labelKey: 'nav.categories', Icon: CategoriesIcon },
  { screen: 'recurring', labelKey: 'nav.recurring', Icon: RecurringIcon },
  { screen: 'stats', labelKey: 'nav.stats', Icon: StatsIcon },
];

export function NavBar() {
  const { t } = useTranslation();
  const { screen, navigate } = useNavigation();

  return (
    <nav className="nav-bar" aria-label={t('nav.sections')}>
      {ITEMS.map(({ screen: target, labelKey, Icon }) => {
        const active = screen === target;
        return (
          <button
            key={target}
            type="button"
            className={`nav-bar__item${active ? ' nav-bar__item--active' : ''}`}
            aria-label={t(labelKey)}
            aria-current={active ? 'page' : undefined}
            onClick={() => navigate(active ? 'month' : target)}
          >
            <Icon />
          </button>
        );
      })}
    </nav>
  );
}

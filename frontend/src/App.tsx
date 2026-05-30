import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';

import { useAccounts, useCreateAccount } from './api/hooks';
import { AccountSwitcher } from './components/AccountSwitcher';
import { ErrorBanner } from './components/ErrorBanner';
import { CogIcon } from './components/icons';
import { NavBar } from './components/NavBar';
import { Spinner } from './components/Spinner';
import { defaultCurrency } from './lib/locale';
import { currentMonth } from './lib/month';
import { ManageAccountsScreen } from './screens/ManageAccountsScreen';
import { MonthScreen } from './screens/MonthScreen';
import { PlaceholderScreen } from './screens/PlaceholderScreen';
import { SettingsScreen } from './screens/SettingsScreen';
import { useActiveAccount, useNavigation } from './state/contexts';
import type { Account } from './api/types';

function CurrentScreen({ active }: { active: Account }) {
  const { screen } = useNavigation();
  switch (screen) {
    case 'accounts':
      return <ManageAccountsScreen />;
    case 'settings':
      return <SettingsScreen />;
    case 'categories':
      return <PlaceholderScreen titleKey="placeholder.categories" />;
    case 'recurring':
      return <PlaceholderScreen titleKey="placeholder.recurring" />;
    case 'stats':
      return <PlaceholderScreen titleKey="placeholder.stats" />;
    default:
      return <MonthScreen account={active} />;
  }
}

function App() {
  const { t } = useTranslation();
  const { data: accounts, isPending, error } = useAccounts();
  const { activeAccountId, setActiveAccountId } = useActiveAccount();
  const { screen, navigate } = useNavigation();
  const bootstrap = useCreateAccount();
  const bootstrapMutate = bootstrap.mutate;
  const bootstrapStarted = useRef(false);

  // First run: create a ready-to-use default account so the user lands straight
  // in the app. Additional accounts are made in Manage Accounts. Fires once.
  useEffect(() => {
    if (!accounts || accounts.length > 0 || bootstrapStarted.current) {
      return;
    }
    bootstrapStarted.current = true;
    bootstrapMutate(
      {
        name: 'Personal',
        icon: '💰',
        currency: defaultCurrency(),
        opening_balance: '0.00',
        anchor: currentMonth(),
      },
      { onSuccess: (account) => setActiveAccountId(account.id) },
    );
  }, [accounts, bootstrapMutate, setActiveAccountId]);

  // Reconcile the persisted active account against the live list.
  useEffect(() => {
    if (!accounts || accounts.length === 0) {
      return;
    }
    const exists = activeAccountId !== null && accounts.some((a) => a.id === activeAccountId);
    if (!exists) {
      setActiveAccountId(accounts[0].id);
    }
  }, [accounts, activeAccountId, setActiveAccountId]);

  if (error) {
    return <ErrorBanner error={error} />;
  }
  if (bootstrap.error) {
    return <ErrorBanner error={bootstrap.error} />;
  }
  if (isPending || !accounts || accounts.length === 0) {
    return <Spinner label={t('app.settingUp')} />;
  }

  const active = accounts.find((a) => a.id === activeAccountId) ?? accounts[0];

  return (
    <div className="app">
      <header className="app__header">
        <button type="button" className="app__title" onClick={() => navigate('month')}>
          {t('app.title')}
        </button>
        <div className="app__header-actions">
          {screen === 'month' && <AccountSwitcher accounts={accounts} />}
          <button
            type="button"
            className="icon-btn"
            aria-label={t('nav.settings')}
            aria-current={screen === 'settings' ? 'page' : undefined}
            onClick={() => navigate('settings')}
          >
            <CogIcon />
          </button>
        </div>
      </header>
      <NavBar />
      <main className="app__content">
        <CurrentScreen active={active} />
      </main>
    </div>
  );
}

export default App;

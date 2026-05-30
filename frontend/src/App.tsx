import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useQueryClient } from '@tanstack/react-query';

import { createCategory } from './api/commands';
import { useAccounts, useCategories, useCreateAccount } from './api/hooks';
import { queryKeys } from './api/queryKeys';
import { AccountSwitcher } from './components/AccountSwitcher';
import { ErrorBanner } from './components/ErrorBanner';
import { CogIcon } from './components/icons';
import { NavBar } from './components/NavBar';
import { Spinner } from './components/Spinner';
import { DEFAULT_CATEGORIES } from './lib/categories';
import { defaultCurrency } from './lib/locale';
import { currentMonth } from './lib/month';
import { CategoryManagerScreen } from './screens/CategoryManagerScreen';
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
      return <CategoryManagerScreen />;
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
  const { data: categories } = useCategories();
  const { activeAccountId, setActiveAccountId } = useActiveAccount();
  const { screen, navigate } = useNavigation();
  const bootstrap = useCreateAccount();
  const bootstrapMutate = bootstrap.mutate;
  const bootstrapStarted = useRef(false);
  const categoriesSeeded = useRef(false);
  const queryClient = useQueryClient();

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

  // First run: seed a set of common categories (labels localized to the current
  // language at seed time). Fires once; not re-seeded if the user later clears them.
  useEffect(() => {
    if (!categories || categories.length > 0 || categoriesSeeded.current) {
      return;
    }
    categoriesSeeded.current = true;
    void (async () => {
      for (const c of DEFAULT_CATEGORIES) {
        try {
          await createCategory({ label: t(c.labelKey), icon: { emoji: c.emoji } });
        } catch (cause) {
          // Best-effort seeding; log but keep going.
          console.warn('Failed to seed category', c.labelKey, cause);
        }
      }
      void queryClient.invalidateQueries({ queryKey: queryKeys.categories });
    })();
    // `t` is a dep so labels seed in the active language; the ref-guard makes any
    // re-run (e.g. on language change) a no-op.
  }, [categories, queryClient, t]);

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

  // Escape returns to the month screen from any sub-screen — unless a modal is
  // open (its own Escape handler takes precedence).
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== 'Escape' || screen === 'month') {
        return;
      }
      if (document.querySelector('.modal-backdrop')) {
        return;
      }
      navigate('month');
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [screen, navigate]);

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
            className={`icon-btn${screen === 'settings' ? ' icon-btn--active' : ''}`}
            aria-label={t('nav.settings')}
            aria-current={screen === 'settings' ? 'page' : undefined}
            onClick={() => navigate(screen === 'settings' ? 'month' : 'settings')}
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

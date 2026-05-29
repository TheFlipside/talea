import { useEffect, useRef } from 'react';

import { useAccounts, useCreateAccount } from './api/hooks';
import { AccountSwitcher } from './components/AccountSwitcher';
import { ErrorBanner } from './components/ErrorBanner';
import { Spinner } from './components/Spinner';
import { MonthScreen } from './screens/MonthScreen';
import { defaultCurrency } from './lib/locale';
import { currentMonth } from './lib/month';
import { useActiveAccount } from './state/contexts';

function App() {
  const { data: accounts, isPending, error } = useAccounts();
  const { activeAccountId, setActiveAccountId } = useActiveAccount();
  const bootstrap = useCreateAccount();
  const bootstrapMutate = bootstrap.mutate;
  const bootstrapStarted = useRef(false);

  // First run: create a ready-to-use default account (currency from the system
  // locale) so the user lands straight in the app. Additional accounts are made
  // via the switcher. Guarded so it fires exactly once.
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
  // Loading accounts, or creating the first one.
  if (isPending || !accounts || accounts.length === 0) {
    return <Spinner label="Setting up…" />;
  }

  const active = accounts.find((a) => a.id === activeAccountId) ?? accounts[0];

  return (
    <div className="app">
      <header className="app__header">
        <h1 className="app__title">Talea</h1>
        <AccountSwitcher accounts={accounts} />
      </header>
      <MonthScreen account={active} />
    </div>
  );
}

export default App;

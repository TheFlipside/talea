import { useEffect } from 'react';

import { useAccounts } from './api/hooks';
import { AccountOnboarding } from './components/AccountOnboarding';
import { AccountSwitcher } from './components/AccountSwitcher';
import { ErrorBanner } from './components/ErrorBanner';
import { Spinner } from './components/Spinner';
import { MonthScreen } from './screens/MonthScreen';
import { useActiveAccount } from './state/contexts';

function App() {
  const { data: accounts, isPending, error } = useAccounts();
  const { activeAccountId, setActiveAccountId } = useActiveAccount();

  // Reconcile the persisted active account against the live list: if it was
  // deleted (or never set), fall back to the first account, or none.
  useEffect(() => {
    if (!accounts) {
      return;
    }
    const exists = activeAccountId !== null && accounts.some((a) => a.id === activeAccountId);
    if (!exists) {
      setActiveAccountId(accounts[0]?.id ?? null);
    }
  }, [accounts, activeAccountId, setActiveAccountId]);

  if (isPending) {
    return <Spinner />;
  }
  if (error) {
    return <ErrorBanner error={error} />;
  }
  if (accounts.length === 0) {
    return (
      <div className="onboarding">
        <AccountOnboarding />
      </div>
    );
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

import { useTranslation } from 'react-i18next';

import type { Account } from '../api/types';
import { useActiveAccount } from '../state/contexts';
import { Select } from './Select';

/** Quick-switch the active account. Add/edit/delete live in Manage Accounts. */
export function AccountSwitcher({ accounts }: { accounts: Account[] }) {
  const { t } = useTranslation();
  const { activeAccountId, setActiveAccountId } = useActiveAccount();

  const options = accounts.map((account) => ({
    value: String(account.id),
    label: (
      <span>
        {account.icon} {account.name} <span className="muted">({account.currency})</span>
      </span>
    ),
    triggerLabel: (
      <span>
        {account.icon} {account.name}
      </span>
    ),
  }));

  return (
    <div className="account-switcher">
      <Select
        value={String(activeAccountId ?? accounts[0]?.id ?? '')}
        options={options}
        onChange={(value) => {
          const id = Number(value);
          if (Number.isInteger(id) && id > 0) {
            setActiveAccountId(id);
          }
        }}
        ariaLabel={t('account.active')}
      />
    </div>
  );
}

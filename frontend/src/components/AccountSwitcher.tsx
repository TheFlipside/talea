import { useState } from 'react';

import type { Account } from '../api/types';
import { useActiveAccount } from '../state/contexts';
import { AccountOnboarding } from './AccountOnboarding';
import { Modal } from './Modal';
import { Select } from './Select';

export function AccountSwitcher({ accounts }: { accounts: Account[] }) {
  const { activeAccountId, setActiveAccountId } = useActiveAccount();
  const [adding, setAdding] = useState(false);

  const options = accounts.map((account) => ({
    value: String(account.id),
    label: (
      <span>
        {account.icon} {account.name}{' '}
        <span className="muted">({account.currency})</span>
      </span>
    ),
  }));

  return (
    <div className="account-switcher">
      <div className="field field--inline">
        <Select
          value={String(activeAccountId ?? accounts[0]?.id ?? '')}
          options={options}
          onChange={(value) => {
            const id = Number(value);
            if (Number.isInteger(id) && id > 0) {
              setActiveAccountId(id);
            }
          }}
          ariaLabel="Active account"
        />
      </div>
      <button type="button" className="btn btn--ghost" onClick={() => setAdding(true)}>
        + Account
      </button>

      {adding && (
        <Modal label="New account" onClose={() => setAdding(false)}>
          <AccountOnboarding allowCancel onDone={() => setAdding(false)} />
        </Modal>
      )}
    </div>
  );
}

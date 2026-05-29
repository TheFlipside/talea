import { useState } from 'react';

import type { Account } from '../api/types';
import { useActiveAccount } from '../state/contexts';
import { AccountOnboarding } from './AccountOnboarding';
import { Modal } from './Modal';

export function AccountSwitcher({ accounts }: { accounts: Account[] }) {
  const { activeAccountId, setActiveAccountId } = useActiveAccount();
  const [adding, setAdding] = useState(false);

  return (
    <div className="account-switcher">
      <label className="field field--inline">
        <span className="visually-hidden">Active account</span>
        <select
          value={activeAccountId ?? ''}
          onChange={(e) => {
            const id = Number(e.currentTarget.value);
            if (Number.isInteger(id) && id > 0) {
              setActiveAccountId(id);
            }
          }}
        >
          {accounts.map((account) => (
            <option key={account.id} value={account.id}>
              {account.icon} {account.name} ({account.currency})
            </option>
          ))}
        </select>
      </label>
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

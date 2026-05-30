import { useTranslation } from 'react-i18next';

import type { AccountId, Entry, NewEntry, Occurrence } from '../api/types';
import { useDetachOccurrence, useSkipOccurrence } from '../api/hooks';
import { formatEntryDate } from '../lib/date';
import { Modal } from './Modal';

interface OccurrenceActionsProps {
  accountId: AccountId;
  occurrence: Occurrence;
  onClose: () => void;
  /** Called with the new standalone entry after a detach, to open its editor. */
  onDetached: (entry: Entry) => void;
}

/** A recurring occurrence copied verbatim into a create-entry payload. */
function occurrenceToEntry(occurrence: Occurrence): NewEntry {
  return {
    account_id: occurrence.account_id,
    amount: occurrence.amount,
    kind: occurrence.kind,
    date: occurrence.date,
    note: occurrence.note ?? null,
    category_id: occurrence.category_id ?? null,
  };
}

/**
 * Actions for a single recurring occurrence: edit just this one (detach it into
 * an editable standalone entry, then open the editor) or remove just this one
 * (skip it). The rule's other months are unaffected.
 */
export function OccurrenceActions({
  accountId,
  occurrence,
  onClose,
  onDetached,
}: OccurrenceActionsProps) {
  const { t } = useTranslation();
  const skip = useSkipOccurrence(accountId);
  const detach = useDetachOccurrence(accountId);
  const busy = skip.isPending || detach.isPending;
  const errorMessage = skip.error?.message ?? detach.error?.message ?? null;

  function handleEdit() {
    skip.reset(); // clear any stale error from a prior action
    detach.mutate(
      { ruleId: occurrence.rule_id, date: occurrence.date, entry: occurrenceToEntry(occurrence) },
      { onSuccess: (entry) => onDetached(entry) },
    );
  }

  function handleDelete() {
    detach.reset();
    skip.mutate({ ruleId: occurrence.rule_id, date: occurrence.date }, { onSuccess: onClose });
  }

  return (
    <Modal label={t('recurring.occurrenceTitle')} onClose={onClose}>
      <h2>{t('recurring.occurrenceTitle')}</h2>
      <p className="confirm-text">
        {t('recurring.occurrenceOn', { date: formatEntryDate(occurrence.date) })}
      </p>

      {errorMessage && <p className="field-error">{errorMessage}</p>}

      <div className="occurrence-actions">
        <button type="button" className="btn" onClick={handleEdit} disabled={busy} data-autofocus="true">
          {t('recurring.editOccurrence')}
        </button>
        <button type="button" className="btn btn--danger" onClick={handleDelete} disabled={busy}>
          {t('recurring.deleteOccurrence')}
        </button>
      </div>
      <div className="modal__actions">
        <span className="modal__spacer" />
        <button type="button" className="btn btn--ghost" onClick={onClose} disabled={busy}>
          {t('common.cancel')}
        </button>
      </div>
    </Modal>
  );
}

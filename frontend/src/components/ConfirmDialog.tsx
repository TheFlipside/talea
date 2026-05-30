import { useTranslation } from 'react-i18next';

import { Modal } from './Modal';

interface ConfirmDialogProps {
  title: string;
  message: string;
  confirmLabel: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/** A confirmation dialog for destructive actions, built on `Modal`. */
export function ConfirmDialog({
  title,
  message,
  confirmLabel,
  busy = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const { t } = useTranslation();
  return (
    <Modal label={title} onClose={onCancel}>
      <div className="confirm">
        <h2>{title}</h2>
        <p>{message}</p>
        <div className="modal__actions">
          <span className="modal__spacer" />
          <button type="button" className="btn btn--ghost" onClick={onCancel} disabled={busy}>
            {t('common.cancel')}
          </button>
          <button type="button" className="btn btn--danger" onClick={onConfirm} disabled={busy}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </Modal>
  );
}

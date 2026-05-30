import { useEffect, useId, useRef, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

// Stack of open modal ids so that, when modals nest, only the topmost responds
// to Escape / Tab (the others ignore the key so they don't all close at once).
const modalStack: string[] = [];

interface ModalProps {
  label: string;
  onClose: () => void;
  children: ReactNode;
}

/**
 * An accessible modal dialog: focus is moved in on open, trapped while open
 * (Tab/Shift+Tab cycle within), restored to the trigger on close, and Escape or
 * a backdrop click closes it.
 */
export function Modal({ label, onClose, children }: ModalProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const id = useId();
  // Hold the latest onClose in a ref so the setup effect can run mount-only and
  // not re-fire (which would steal focus) when the parent passes a new inline
  // onClose on every render.
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    modalStack.push(id);
    return () => {
      const index = modalStack.lastIndexOf(id);
      if (index !== -1) {
        modalStack.splice(index, 1);
      }
    };
  }, [id]);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) {
      return;
    }
    const previouslyFocused = document.activeElement as HTMLElement | null;
    const focusable = () => Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE));

    // Prefer an explicitly marked field, else the first focusable, else the dialog.
    const preferred = dialog.querySelector<HTMLElement>('[data-autofocus]');
    (preferred ?? focusable()[0] ?? dialog).focus();

    function onKeyDown(event: KeyboardEvent) {
      // Only the topmost modal handles keys, so nested modals don't all react.
      if (modalStack[modalStack.length - 1] !== id) {
        return;
      }
      if (event.key === 'Escape') {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== 'Tab') {
        return;
      }
      const items = focusable();
      if (items.length === 0) {
        event.preventDefault();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }

    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      previouslyFocused?.focus?.();
    };
    // `id` is stable (useId), so this runs once on mount — it does not re-fire
    // and steal focus on parent re-renders.
  }, [id]);

  // Portal to <body> so the modal always stacks above the app content,
  // regardless of where in the tree it is rendered from.
  return createPortal(
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div className="modal" role="dialog" aria-modal="true" aria-label={label} ref={dialogRef} tabIndex={-1}>
        {children}
      </div>
    </div>,
    document.body,
  );
}

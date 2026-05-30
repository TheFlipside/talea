import { useEffect, useRef, useState, type KeyboardEvent } from 'react';

import { CATEGORY_EMOJIS } from '../lib/categories';

interface EmojiPickerProps {
  value: string;
  onChange: (emoji: string) => void;
  ariaLabel: string;
}

/** A compact emoji picker: a trigger showing the current emoji and a popup grid.
 * Closes on outside-click or Escape (without closing an enclosing modal). */
export function EmojiPicker({ value, onChange, ariaLabel }: EmojiPickerProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const popupRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    // Move focus into the grid so keyboard users can reach the emojis.
    popupRef.current?.querySelector('button')?.focus();
    function onDocMouseDown(event: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener('mousedown', onDocMouseDown);
    return () => document.removeEventListener('mousedown', onDocMouseDown);
  }, [open]);

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape' && open) {
      event.stopPropagation();
      event.nativeEvent.stopImmediatePropagation();
      event.preventDefault();
      setOpen(false);
    }
  }

  return (
    <div className="emoji-picker" ref={rootRef} onKeyDown={onKeyDown}>
      <button
        type="button"
        className="select__trigger"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={ariaLabel}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="emoji-picker__current">{value}</span>
        <span className="select__caret" aria-hidden="true">
          ▾
        </span>
      </button>

      {open && (
        <div className="emoji-picker__popup" role="dialog" aria-label={ariaLabel} ref={popupRef}>
          <div className="emoji-picker__grid">
            {CATEGORY_EMOJIS.map((emoji) => (
              <button
                key={emoji}
                type="button"
                className={`emoji-picker__item${emoji === value ? ' emoji-picker__item--selected' : ''}`}
                aria-pressed={emoji === value}
                onClick={() => {
                  onChange(emoji);
                  setOpen(false);
                }}
              >
                {emoji}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

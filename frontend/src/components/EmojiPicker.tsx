import { useState } from 'react';

import { CATEGORY_EMOJIS } from '../lib/categories';
import { Modal } from './Modal';

interface EmojiPickerProps {
  value: string;
  onChange: (emoji: string) => void;
  /** Accessible label / dialog title, e.g. "Choose an icon". */
  ariaLabel: string;
}

/** A trigger showing the current emoji that opens a modal with a grid of
 * emojis to choose from. */
export function EmojiPicker({ value, onChange, ariaLabel }: EmojiPickerProps) {
  const [open, setOpen] = useState(false);

  return (
    <>
      <button
        type="button"
        className="select__trigger"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={ariaLabel}
        onClick={() => setOpen(true)}
      >
        <span className="emoji-picker__current">{value}</span>
        <span className="select__caret" aria-hidden="true">
          ▾
        </span>
      </button>

      {open && (
        <Modal label={ariaLabel} onClose={() => setOpen(false)}>
          <h2>{ariaLabel}</h2>
          <div className="emoji-grid">
            {CATEGORY_EMOJIS.map((emoji) => (
              <button
                key={emoji}
                type="button"
                className={`emoji-grid__item${emoji === value ? ' emoji-grid__item--selected' : ''}`}
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
        </Modal>
      )}
    </>
  );
}

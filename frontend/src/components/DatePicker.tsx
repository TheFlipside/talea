import { useEffect, useRef, useState, type KeyboardEvent } from 'react';

import type { IsoDate } from '../api/types';
import { formatFullDate, isoDate, isoDayOf, isoMonthOf } from '../lib/date';
import { monthLabel, nextMonth, prevMonth } from '../lib/month';

interface DatePickerProps {
  value: IsoDate;
  onChange: (value: IsoDate) => void;
  ariaLabel: string;
}

// Localized short weekday names, Monday-first (2024-01-01 was a Monday).
const WEEKDAYS = (() => {
  const fmt = new Intl.DateTimeFormat(undefined, { weekday: 'short' });
  return [1, 2, 3, 4, 5, 6, 7].map((d) => fmt.format(new Date(2024, 0, d)));
})();

/** Days in a month (month is 1..=12). */
function daysInMonth(year: number, month: number): number {
  return new Date(year, month, 0).getDate();
}

/** Monday-first index (0..6) of the first day of a month. */
function leadingBlanks(year: number, month: number): number {
  return (new Date(year, month - 1, 1).getDay() + 6) % 7;
}

/**
 * A custom date picker: a styled calendar popup that closes on outside-click or
 * Escape, so it dismisses consistently on desktop and touch (unlike the native
 * `<input type="date">` popup).
 */
export function DatePicker({ value, onChange, ariaLabel }: DatePickerProps) {
  const [open, setOpen] = useState(false);
  const [view, setView] = useState(() => isoMonthOf(value));
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    function onDocMouseDown(event: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener('mousedown', onDocMouseDown);
    return () => document.removeEventListener('mousedown', onDocMouseDown);
  }, [open]);

  function openCalendar() {
    setView(isoMonthOf(value));
    setOpen(true);
  }

  function pick(day: number) {
    onChange(isoDate(view.year, view.month, day));
    setOpen(false);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape' && open) {
      // Close only the calendar — not an enclosing modal (whose listener is on
      // `document`, so stop the native event from reaching it).
      event.stopPropagation();
      event.nativeEvent.stopImmediatePropagation();
      event.preventDefault();
      setOpen(false);
    }
  }

  const selected = isoMonthOf(value);
  const selectedDay = isoDayOf(value);
  const blanks = leadingBlanks(view.year, view.month);
  const total = daysInMonth(view.year, view.month);
  const cells: (number | null)[] = [
    ...Array.from({ length: blanks }, () => null),
    ...Array.from({ length: total }, (_, i) => i + 1),
  ];

  return (
    <div className="datepicker" ref={rootRef} onKeyDown={onKeyDown}>
      <button
        type="button"
        className="select__trigger"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={ariaLabel}
        onClick={() => (open ? setOpen(false) : openCalendar())}
      >
        <span className="select__value">{formatFullDate(value)}</span>
        <span className="select__caret" aria-hidden="true">
          📅
        </span>
      </button>

      {open && (
        <div className="datepicker__popup">
          <div className="datepicker__nav">
            <button
              type="button"
              className="month-nav__arrow"
              aria-label="Previous month"
              onClick={() => setView((v) => prevMonth(v))}
            >
              ‹
            </button>
            <span className="datepicker__label">{monthLabel(view)}</span>
            <button
              type="button"
              className="month-nav__arrow"
              aria-label="Next month"
              onClick={() => setView((v) => nextMonth(v))}
            >
              ›
            </button>
          </div>

          <div className="datepicker__grid" role="grid">
            {WEEKDAYS.map((w) => (
              <span key={w} className="datepicker__weekday" aria-hidden="true">
                {w}
              </span>
            ))}
            {cells.map((day, index) =>
              day === null ? (
                <span key={`blank-${index}`} />
              ) : (
                <button
                  key={day}
                  type="button"
                  className={`datepicker__day${
                    selected.year === view.year && selected.month === view.month && day === selectedDay
                      ? ' datepicker__day--selected'
                      : ''
                  }`}
                  aria-pressed={
                    selected.year === view.year && selected.month === view.month && day === selectedDay
                  }
                  onClick={() => pick(day)}
                >
                  {day}
                </button>
              ),
            )}
          </div>
        </div>
      )}
    </div>
  );
}

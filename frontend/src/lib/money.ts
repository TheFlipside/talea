/**
 * Display-only money formatting.
 *
 * Money values are strings everywhere; the only place we turn one into a number
 * is here (and the budget ring), strictly for presentation. Never feed these
 * numbers back into a displayed amount — the arithmetic of record lives in the
 * Rust core.
 */

import type { Money } from '../api/types';

/**
 * Whether `text` is a well-formed decimal money string (optional leading `-`,
 * digits, optional fractional part). Used to catch invalid input in the UI
 * before it reaches the backend — where a bad value would otherwise fail at the
 * deserialization layer and surface as an opaque error.
 */
export function isMoneyInput(text: string): boolean {
  return /^-?\d+(\.\d+)?$/.test(text.trim());
}

const formatterCache = new Map<string, Intl.NumberFormat>();

function currencyFormatter(currency: string): Intl.NumberFormat | null {
  const cached = formatterCache.get(currency);
  if (cached) {
    return cached;
  }
  try {
    const formatter = new Intl.NumberFormat(undefined, {
      style: 'currency',
      currency,
    });
    formatterCache.set(currency, formatter);
    return formatter;
  } catch {
    // `Intl` throws on a code it doesn't recognize, even if well-formed.
    return null;
  }
}

/**
 * Formats a money string in the given currency, e.g. `("12.34", "USD") → "$12.34"`.
 * Falls back to a plain number + code for unknown currencies, and to the raw
 * string if the amount is not parseable.
 */
export function formatMoney(amount: Money, currency: string): string {
  const value = Number(amount);
  if (Number.isNaN(value)) {
    return amount;
  }
  const formatter = currencyFormatter(currency);
  if (formatter) {
    return formatter.format(value);
  }
  return `${new Intl.NumberFormat(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(value)} ${currency}`;
}

/**
 * Parses a money string to a number for *visual ratios only* (the budget ring).
 * The single audited `Number()` chokepoint outside `formatMoney`.
 */
export function parseMoneyForDisplay(amount: Money): number {
  const value = Number(amount);
  return Number.isNaN(value) ? 0 : value;
}

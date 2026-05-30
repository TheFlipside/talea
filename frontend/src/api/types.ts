/**
 * TypeScript mirrors of the Rust domain's JSON wire shapes.
 *
 * Field names are deliberately snake_case to match serde exactly (no mapping
 * layer). Key invariants:
 * - IDs are bare integers (`#[serde(transparent)]` over u64).
 * - Money is always a STRING (e.g. "12.34"); never parse it for math — money
 *   arithmetic lives in the Rust core. Parse only for display formatting.
 * - Dates are ISO `YYYY-MM-DD` strings.
 * - `Entry.note` / `category_id` may be ABSENT (the domain uses
 *   `skip_serializing_if`), so they are optional *and* nullable.
 */

export type AccountId = number;
export type EntryId = number;
export type CategoryId = number;

/** A decimal money amount as a string. Never a JS number. */
export type Money = string;
/** An ISO-8601 calendar date, `YYYY-MM-DD`. */
export type IsoDate = string;

export type EntryKind = 'income' | 'expense';

export interface Month {
  year: number;
  /** 1..=12 */
  month: number;
}

export interface Account {
  id: AccountId;
  name: string;
  icon: string;
  currency: string;
  opening_balance: Money;
  anchor: Month;
}

export interface NewAccount {
  name: string;
  icon: string;
  currency: string;
  opening_balance: Money;
  anchor: Month;
}

export interface Entry {
  id: EntryId;
  account_id: AccountId;
  amount: Money;
  kind: EntryKind;
  date: IsoDate;
  note?: string | null;
  category_id?: CategoryId | null;
}

export interface NewEntry {
  account_id: AccountId;
  amount: Money;
  kind: EntryKind;
  date: IsoDate;
  note?: string | null;
  category_id?: CategoryId | null;
}

/** A category's marker: a preset id or a literal emoji (UI uses emoji). */
export type CategoryIcon = { preset: string } | { emoji: string };

export interface Category {
  id: CategoryId;
  label: string;
  icon: CategoryIcon;
}

export interface NewCategory {
  label: string;
  icon: CategoryIcon;
}

export interface MonthSummary {
  month: Month;
  carry_in: Money;
  income: Money;
  expenses: Money;
  available: Money;
}

export type CommandErrorCode = 'validation' | 'not_found' | 'database' | 'corrupt';

export interface CommandError {
  code: CommandErrorCode;
  message: string;
}

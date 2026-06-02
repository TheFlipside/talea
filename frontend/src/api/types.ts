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
export type RecurringRuleId = number;

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

/** Whether an account records its own entries (`normal`) or is a read-only
 *  overview aggregating same-currency members (`summary`). */
export type AccountKind = 'normal' | 'summary';

export interface Account {
  id: AccountId;
  name: string;
  icon: string;
  currency: string;
  opening_balance: Money;
  anchor: Month;
  kind: AccountKind;
  /** Member account ids — non-empty only for a summary account. */
  members: AccountId[];
}

export interface NewAccount {
  name: string;
  icon: string;
  currency: string;
  opening_balance: Money;
  anchor: Month;
  kind: AccountKind;
  members: AccountId[];
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

export type FreqUnit = 'weekly' | 'monthly' | 'yearly';

/** A cadence: a unit repeated every `interval` units (`interval >= 1`). */
export interface Frequency {
  unit: FreqUnit;
  interval: number;
}

/** When a recurring rule stops. Wire shape is internally tagged by `kind`. */
export type RuleEnd = { kind: 'never' } | { kind: 'until'; date: IsoDate };

/** One step in a rule's amount history: `amount` from `effective_from` onward. */
export interface AmountSegment {
  effective_from: IsoDate;
  amount: Money;
}

export interface RecurringRule {
  id: RecurringRuleId;
  account_id: AccountId;
  /** Amount history; the first segment is the base at `start_date`. Never empty. */
  amounts: AmountSegment[];
  kind: EntryKind;
  note?: string | null;
  category_id?: CategoryId | null;
  start_date: IsoDate;
  end: RuleEnd;
  frequency: Frequency;
}

/** Create payload: a rule starts with a single base amount (effective at start). */
export interface NewRule {
  account_id: AccountId;
  amount: Money;
  kind: EntryKind;
  note?: string | null;
  category_id?: CategoryId | null;
  start_date: IsoDate;
  end: RuleEnd;
  frequency: Frequency;
}

/**
 * A recurring rule expanded into a single month occurrence (read-only; not
 * stored). Mirrors the core `VirtualEntry`: it has no entry id, but carries the
 * `rule_id` it came from.
 */
export interface Occurrence {
  rule_id: RecurringRuleId;
  account_id: AccountId;
  amount: Money;
  kind: EntryKind;
  date: IsoDate;
  note?: string | null;
  category_id?: CategoryId | null;
}

export interface MonthSummary {
  month: Month;
  carry_in: Money;
  income: Money;
  expenses: Money;
  available: Money;
}

/**
 * One category's total expense within a month (a positive money string).
 * `category_id` is `null` for uncategorized expenses (shown as "Other").
 */
export interface CategoryExpense {
  category_id: CategoryId | null;
  total: Money;
}

/**
 * The frontend-visible Nextcloud backup settings. Mirrors the Rust
 * `NextcloudConfigView` (camelCase). The password is deliberately absent — the
 * backend never returns it; `configured` reports whether one is stored.
 */
export interface NextcloudConfigView {
  baseUrl: string;
  username: string;
  configured: boolean;
  /** RFC-3339 timestamp of the last successful backup, or null if none. */
  lastBackup: string | null;
}

export type CommandErrorCode = 'validation' | 'not_found' | 'database' | 'corrupt' | 'backup';

export interface CommandError {
  code: CommandErrorCode;
  message: string;
}

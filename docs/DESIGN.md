# Talea — Design Decisions

Canonical record of product/architecture decisions and the few details still
open. Status legend: 🔴 open · 🟡 leaning · 🟢 decided.

---

## 1. Budgeting model — 🟢 DECIDED: monthly cashflow ledger with carry-over

Talea is a **month-focused cashflow ledger**, not envelope budgeting and not
per-category limits.

- The unit of attention is **the current month and upcoming months**. The main
  screen shows, for the selected month, a top bar with **income, expenses, and
  the resulting available budget to end of month**, followed by the list of
  recorded entries. Swiping moves between months.
- **Available to end of month** = `carry-in + Σ income(month) − Σ expenses(month)`,
  where entries include both ad-hoc recordings **and** the expansion of recurring
  rules that apply to that month.
- **Carry-over is ON** (decided): a month's ending balance flows into the next
  month as `carry-in`. A surplus raises next month; an overspend reduces it.
  Months therefore form a **chain** per account.
- **Categories are descriptive**, used for classification and the stats screen —
  **not** funded envelopes and **not** spending limits. No per-category budget.
- A **"month" is a derived view** over dated entries, never a stored allocation
  row. The running balance is a cumulative sum, so no per-month table is needed.

### Carry-over chain & opening balance — 🟢 DECIDED

With carry-over on, the chain needs a starting point (confirmed):

- Each **Account** has an **`opening_balance`** (`Money`, default `0`) effective
  as of an **`anchor` month** (default: the account's creation month).
- Balance at end of month *M* = `opening_balance + Σ(entries dated ≤ end of M)`,
  income positive, expenses negative, recurring expanded. "Available this month"
  is that same running figure evaluated at month end.

---

## 2. Entities & relationships — 🟢 DECIDED (model), schema in §3

```
Account (1) ─── (N) Entry
Account (1) ─── (N) RecurringRule
Category (1) ─── (N) Entry           # category is optional on an entry
Category (1) ─── (N) RecurringRule   # categories are GLOBAL (shared by accounts)
```

- **Account** — `id`, `name`, `icon`, **`currency`** (fixed ISO 4217, per
  account), `opening_balance: Money`, `anchor` month. Multiple accounts; user
  switches between them. Data is scoped per account; **no cross-account
  aggregation or conversion** (see §6).
- **Category** — `id`, `label`, `icon` (a preset id **or** an emoji). **Global**:
  one list shared across all accounts, maintained on its own screen. Applies to
  both income and expense entries; the stats screen aggregates expenses by it.
- **Entry** — `id`, `account_id`, `amount: Money` (positive magnitude),
  `kind: Income | Expense`, `date`, `note: Option<String>`,
  `category_id: Option<Id>`. Full CRUD (add / edit / delete).
- **RecurringRule** — an entry template that is **dateless** but has
  `start_date`, `end: Never | Until(date)`, and a **frequency** (§4). It is
  *expanded* into virtual entries for each month it covers; expansions are
  computed, not stored (editing the rule re-derives them). Same fields as Entry
  otherwise (`account_id`, `amount`, `kind`, `note`, `category_id`).

`Month { year, month }` remains a value type used to window queries, not a row.

---

## 3. SQLite schema — 🔴 NEXT (now unblocked by §1–§2)

To be written in `src-tauri` (tables, keys, indices, `sqlx` migrations) from the
model above. `core` never sees a connection — it receives plain types. Expected
tables: `account`, `category`, `entry`, `recurring_rule`. Money stored as **TEXT**
(decimal string), dates as **TEXT** ISO-8601 (or integer epoch — to decide with
the migration). Index `entry(account_id, date)` for month-window queries.

---

## 4. Recurrence — 🟢 DECIDED: configurable intervals

Rules support **weekly / monthly / yearly**, with an **every-N** multiplier
(e.g. every 2 weeks). Anchored to `start_date`; an explicit `end` of `Never` or
`Until(date)`.

Edge cases to handle in the expansion logic (not the schema):

- **Month-end clamping:** a monthly rule starting on the 31st must resolve to the
  last valid day in shorter months (28/29/30).
- **Yearly Feb-29:** clamp to Feb-28 in non-leap years.
- Expansion is bounded by the queried month window; never enumerate to infinity.

---

## 5. Money & currency — 🟢 DECIDED

- All monetary values use `rust_decimal::Decimal`. **Floating point is forbidden**
  everywhere, including serialization and the frontend boundary, where money
  crosses as **strings**, never JSON numbers. See `core::money`.
- **Per-account currency:** each account stores a fixed ISO 4217 code. No FX
  conversion and no cross-account totals in v1. Display formats to the currency's
  minor units (rounding via `Money::round_dp`).

---

## 6. Home-screen widget — 🟢 DECIDED (later milestone)

Shows only an **abstract ring (spent / remaining) and/or a percentage** of budget
health. **Absolute figures never leave the core app** — only the percentage /
ring fraction is published to the OS shared storage the widget reads. Actual
numbers stay in-app, behind the optional biometric lock. Later milestone, not
part of the current scaffold.

---

## 7. App lock — 🟢 DECIDED (later milestone)

Access to the app is **optionally** gated behind device biometrics. Whole-app
lock (not per-screen). Later milestone.

---

## 8. Domain validation & input limits — 🟢 DONE (implemented in `core`)

Implemented with the §2 types:

- **Validated constructors, private fields.** `Month::new` rejects months
  outside `1..=12`; typed IDs (`AccountId`/…/`RecurringRuleId`) wrap a private
  `u64`. Every validated type uses `#[serde(try_from = "…Repr")]` so malformed
  JSON (over IPC or storage) is rejected at deserialize, not silently accepted.
- **String length caps.** `note` ≤ `MAX_NOTE_LEN` (1000), labels/names ≤
  `MAX_LABEL_LEN` (200), counted in characters. Currency validated to a 3-letter
  ISO code.
- **Amount sign discipline.** `Entry`/`RecurringRule` amounts are positive
  magnitudes; the sign is derived from `EntryKind`. Zero/negative is rejected.

---

## 9. Security hardening backlog (tracked, non-blocking)

Accepted as known debt for the scaffold; revisit before a release:

- **CSP `style-src 'unsafe-inline'`.** Kept because Vite's pipeline may emit
  inline styles; `script-src` is already `'self'` (the XSS-critical one). Tighten
  to `'self'` via hashes/nonces once the production asset pipeline is settled and
  runtime-verified.
- **`core:event` emit scope.** `core:default` grants `allow-emit`/`allow-emit-to`.
  Harmless for one window; narrow to listen-only if a second webview is added.
- **`panic = "abort"`** (release profile) turns any handler panic into a hard
  crash. Mitigated by bounding IPC inputs and preferring `Money::checked_*`; keep
  that discipline as commands are added.
- **Ledger arithmetic uses `Money`'s `+`/`-`** (panic-on-overflow, by design,
  consistent with `money.rs`). Overflow is made unreachable in practice by two
  caps: `Month` years are bounded to `1..=9999` (so the carry-over walk length
  is bounded and date math can't panic), and entry/rule amounts are capped at one
  quadrillion — far below `Decimal::MAX`. If a stricter guarantee is ever wanted,
  convert the ledger functions to return `Result` with checked arithmetic. Note
  the ledger is O(history); the persistence layer may cache per-month aggregates.

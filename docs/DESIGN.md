# Talea — Design Decisions

This document tracks **open design decisions** that must be settled before the
corresponding code is finalized. It is the canonical place for "we deliberately
have not decided this yet" notes.

Status legend: 🔴 open · 🟡 leaning · 🟢 decided

---

## 1. Budgeting model — 🔴 OPEN (blocks everything below)

**Decision needed before:** finalizing the domain model in `core` and the SQLite
schema in `src-tauri`.

The choice of budgeting paradigm determines the relationships between the four
core entities — **month, category, budget, transaction** — and therefore the
entire data model. Do **not** invent this on the fly; it is a deliberate product
decision.

Options under consideration:

- **Envelope (zero-based / "give every dollar a job").**
  Each month you allocate available money into category "envelopes". Spending
  draws down an envelope; overspend is visible and must be covered. Carry-over
  rules (does a leftover envelope roll into next month?) are part of this model.
  - Implies: a `budget` (allocation) row per (month, category); transactions
    reduce the remaining envelope balance; explicit handling of unallocated and
    overspent amounts.

- **Flexible (target/limit-based).**
  Categories have monthly targets or limits; spending is compared against the
  target but money is not physically moved into envelopes. Simpler mental model,
  weaker "every dollar assigned" guarantee.
  - Implies: `budget` is a target per (month, category); no envelope balance to
    carry; reporting is "spent vs. target".

- **Hybrid.**
  Envelope semantics for some categories, flexible targets for others. Most
  flexible for users, most complex to model and to explain.

### Open sub-questions (depend on the choice above)

- **Carry-over:** do unspent/overspent category balances roll to next month?
  Per-category opt-in?
- **Month boundary:** strict calendar month, or user-defined pay-period cycles?
- **Multi-currency:** single base currency for v1, or multi-currency from the
  start? (Money is `rust_decimal::Decimal` regardless — see decision 3.)
- **Income handling:** is income just a transaction, or a first-class
  "to be budgeted" pool (envelope-style)?

**Until this is decided:** the `core` domain types are stubbed and marked with
`DESIGN DECISION:` comments. The SQLite schema is intentionally not written.

---

## 2. SQLite schema — 🔴 BLOCKED on decision 1

Will be designed (tables, keys, migrations via `sqlx`) once the budgeting model
is chosen. Persistence lives exclusively in `src-tauri`; `core` never sees a
connection.

---

## 3. Money representation — 🟢 DECIDED

All monetary values use `rust_decimal::Decimal`. **Floating point is forbidden**
for money anywhere — including serialization and the frontend boundary, where
money crosses as **strings**, never JSON numbers. See `core::money`.

---

## 4. Home-screen widget — 🟢 DECIDED (later milestone)

The widget displays only an **abstract ring / color** indicator of budget
health. Actual figures never appear on the widget; they stay **in-app behind a
biometric lock**. This is a later milestone and is intentionally out of scope
for the initial scaffold.

---

## 5. Domain validation & input limits — 🔴 TO DO with the model

Raised by the scaffold's `/review` and `/security-audit` and **deliberately
deferred** because they shape (and are shaped by) the still-open budgeting-model
decision (§1). Do these when the model is finalized, **before** persistence and
the IPC commands that write it land:

- **`Month.month` range.** Currently `u8` with no enforcement; `0` and `13..=255`
  deserialize fine. Add a validated `Month::new` (reject outside `1..=12`) and
  make fields private.
- **`Id` opacity.** `Id(pub u64)` lets callers fabricate/mutate IDs. Make the
  inner field private with an accessor once the ID strategy is chosen.
- **String length caps.** `Category::name` and `Transaction::note` are unbounded
  `String`s that will be persisted and rendered. Add domain constructors that cap
  length (e.g. name ≤ 200, note ≤ 1000) so a giant value can't arrive over IPC
  and reach SQLite. The `core` types must do this — the `Money` field already
  bounds its echoed parse error.

## 6. Security hardening backlog (tracked, non-blocking)

Accepted as known debt for the scaffold; revisit before shipping a release:

- **CSP `style-src 'unsafe-inline'`.** Kept because Vite's pipeline may emit
  inline styles; `script-src` is already `'self'` (the XSS-critical one). Tighten
  `style-src` to `'self'` via hashes/nonces once the production asset pipeline is
  settled and runtime-verified.
- **`core:event` emit scope.** `core:default` grants `allow-emit`/`allow-emit-to`.
  Harmless for one window; narrow to listen-only if a second webview is added.
- **`panic = "abort"`** (release profile) turns any handler panic into a hard
  crash. Mitigated by bounding IPC inputs and preferring `Money::checked_*`;
  keep that discipline as commands are added.

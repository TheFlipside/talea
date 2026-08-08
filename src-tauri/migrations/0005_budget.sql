-- Category budgets: a planned monthly amount for one category, consumed by that
-- month's real spending in the same category (see DESIGN.md §12).
--
-- A budget never moves money and never touches the carry-over chain. It only
-- reserves part of a month's `available`: reserved = max(0, planned - actual),
-- free = available - reserved. Unspent budget expires at month end and stays
-- ordinary surplus, so `available` keeps its exact meaning.
--
-- The amount lives directly on `budget.amount`, effective from the budget's
-- start month -- deliberately the same shape `recurring_rule.amount` had in
-- 0001, so a future `budget_amount` table can add forward-dated breakpoints the
-- way 0002_rule_amount.sql did for rules, without rewriting this table.
--
-- Conventions match 0001_init.sql: Money is TEXT (a decimal string), months are
-- a (year, month) INTEGER pair as on `account.anchor_*`, the table is STRICT.

CREATE TABLE budget (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id  INTEGER NOT NULL
        REFERENCES account (id) ON DELETE CASCADE,
    -- CASCADE, not SET NULL: a budget names exactly one category, and a budget
    -- with no category has nothing left to measure against.
    category_id INTEGER NOT NULL
        REFERENCES category (id) ON DELETE CASCADE,
    amount      TEXT    NOT NULL,                     -- positive magnitude, Money string
    start_year  INTEGER NOT NULL CHECK (start_year  BETWEEN 1 AND 9999),
    start_month INTEGER NOT NULL CHECK (start_month BETWEEN 1 AND 12),
    end_year    INTEGER          CHECK (end_year    BETWEEN 1 AND 9999),
    end_month   INTEGER          CHECK (end_month   BETWEEN 1 AND 12),
    -- Both end columns are set, or neither (an open-ended budget).
    CHECK ((end_year IS NULL) = (end_month IS NULL)),
    -- At most one budget per category per account. Also the account-scoped
    -- lookup index, so only the category side needs one of its own.
    UNIQUE (account_id, category_id)
) STRICT;

CREATE INDEX idx_budget_category ON budget (category_id);

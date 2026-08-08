-- Amount history for category budgets (DESIGN.md §12).
--
-- A budget's base amount stays on `budget.amount`, effective from the budget's
-- start month. This table holds *additional* breakpoints: from each
-- `(effective_year, effective_month)` (strictly after the budget's start) the
-- planned amount becomes `amount`, until a later breakpoint supersedes it.
--
-- Same shape and rationale as 0002_rule_amount.sql, which did this for recurring
-- rules: it lets a plan change going forward without rewriting the past. The
-- stakes differ, though. A rule's amount feeds the ledger, so a retroactive edit
-- would move historical balances; a budget's amount only feeds `reserved`, which
-- never enters the carry-over chain — so a retroactive edit here misreports what
-- past months were planned at rather than corrupting them. Still wrong to show,
-- and it is why a budget's start month is read-only.
--
-- Months are a (year, month) INTEGER pair as on `budget.start_*` and
-- `account.anchor_*`; Money is TEXT (decimal string); the table is STRICT.

CREATE TABLE budget_amount (
    budget_id       INTEGER NOT NULL
        REFERENCES budget (id) ON DELETE CASCADE,
    effective_year  INTEGER NOT NULL CHECK (effective_year  BETWEEN 1 AND 9999),
    effective_month INTEGER NOT NULL CHECK (effective_month BETWEEN 1 AND 12),
    amount          TEXT    NOT NULL,          -- positive magnitude, Money string
    -- Also the (budget, month) lookup index, and it makes a duplicated month
    -- impossible — every month must resolve to exactly one amount.
    PRIMARY KEY (budget_id, effective_year, effective_month)
) STRICT;

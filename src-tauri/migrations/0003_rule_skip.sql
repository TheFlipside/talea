-- Per-occurrence removals ("skips") for recurring rules.
--
-- Recurring occurrences are computed on demand, never stored. To let a user
-- remove or edit a single occurrence without affecting the rule's other months,
-- the removed occurrence's date is recorded here; the expansion omits it. An
-- "edit one occurrence" is a skip plus a normal standalone entry carrying the
-- edited values (which then lives independently of the rule).
--
-- Conventions match the existing schema: dates are ISO `YYYY-MM-DD` TEXT, table
-- is STRICT, and the row is removed when its rule is deleted.

CREATE TABLE rule_skip (
    rule_id         INTEGER NOT NULL
        REFERENCES recurring_rule (id) ON DELETE CASCADE,
    occurrence_date TEXT    NOT NULL,           -- ISO YYYY-MM-DD
    PRIMARY KEY (rule_id, occurrence_date)
) STRICT;

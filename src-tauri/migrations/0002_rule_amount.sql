-- Amount history for recurring rules.
--
-- A rule's base amount stays on `recurring_rule.amount`, effective from the
-- rule's `start_date`. This table holds *additional* breakpoints: from each
-- `effective_from` (which is strictly after the rule's start date) the rule's
-- amount becomes `amount`, until a later breakpoint supersedes it. This lets an
-- amount change going forward (e.g. a raise) without rewriting past months —
-- essential because the ledger chains carry-over and a retroactive change would
-- alter historical balances.
--
-- Conventions match 0001_init.sql: Money is TEXT (decimal string), dates are
-- ISO `YYYY-MM-DD` TEXT, table is STRICT.

CREATE TABLE rule_amount (
    rule_id        INTEGER NOT NULL
        REFERENCES recurring_rule (id) ON DELETE CASCADE,
    effective_from TEXT    NOT NULL,           -- ISO YYYY-MM-DD, > rule.start_date
    amount         TEXT    NOT NULL,           -- positive magnitude, Money string
    PRIMARY KEY (rule_id, effective_from)
) STRICT;

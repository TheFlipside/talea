-- Talea initial schema.
--
-- Conventions (mirroring the `talea-core` domain):
--   * Money is TEXT (a decimal string) -- never a float.
--   * Dates are TEXT, ISO-8601 `YYYY-MM-DD`.
--   * IDs are INTEGER PRIMARY KEY AUTOINCREMENT.
--   * Enum-like columns are TEXT with CHECKs using the exact lowercase tokens
--     the domain's serde `rename_all = "snake_case"` produces.
-- Tables are STRICT (SQLite >= 3.37) for column-type enforcement.

CREATE TABLE account (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT    NOT NULL,
    icon            TEXT    NOT NULL,
    currency        TEXT    NOT NULL,                 -- ISO 4217, 3 letters
    opening_balance TEXT    NOT NULL,                 -- Money decimal string
    anchor_year     INTEGER NOT NULL CHECK (anchor_year BETWEEN 1 AND 9999),
    anchor_month    INTEGER NOT NULL CHECK (anchor_month BETWEEN 1 AND 12)
) STRICT;

CREATE TABLE category (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    label      TEXT NOT NULL,
    icon_kind  TEXT NOT NULL CHECK (icon_kind IN ('preset', 'emoji')),
    icon_value TEXT NOT NULL                          -- preset id or emoji literal
) STRICT;

CREATE TABLE entry (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id  INTEGER NOT NULL
        REFERENCES account (id) ON DELETE CASCADE,
    amount      TEXT    NOT NULL,                      -- positive magnitude, Money string
    kind        TEXT    NOT NULL CHECK (kind IN ('income', 'expense')),
    date        TEXT    NOT NULL,                      -- ISO YYYY-MM-DD
    note        TEXT,                                  -- nullable
    category_id INTEGER
        REFERENCES category (id) ON DELETE SET NULL
) STRICT;

CREATE TABLE recurring_rule (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id    INTEGER NOT NULL
        REFERENCES account (id) ON DELETE CASCADE,
    amount        TEXT    NOT NULL,
    kind          TEXT    NOT NULL CHECK (kind IN ('income', 'expense')),
    note          TEXT,
    category_id   INTEGER
        REFERENCES category (id) ON DELETE SET NULL,
    start_date    TEXT    NOT NULL,                    -- ISO YYYY-MM-DD
    end_kind      TEXT    NOT NULL CHECK (end_kind IN ('never', 'until')),
    end_date      TEXT,                                -- non-null iff end_kind = 'until'
    freq_unit     TEXT    NOT NULL
        CHECK (freq_unit IN ('weekly', 'monthly', 'yearly')),
    freq_interval INTEGER NOT NULL CHECK (freq_interval >= 1),
    CHECK ((end_kind = 'until') = (end_date IS NOT NULL))
) STRICT;

CREATE INDEX idx_entry_account_date ON entry (account_id, date);
CREATE INDEX idx_entry_category     ON entry (category_id);
CREATE INDEX idx_rule_account       ON recurring_rule (account_id);
CREATE INDEX idx_rule_category      ON recurring_rule (category_id);

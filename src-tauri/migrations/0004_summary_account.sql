-- Summary accounts: a read-only account kind that aggregates several normal
-- accounts of the same currency into a combined month view (see DESIGN.md §11).
--
-- `kind` distinguishes a normal account (records its own entries/rules) from a
-- summary account (records nothing; its figures are derived from its members).
-- Existing rows predate summaries, so the column defaults to 'normal'.
--
-- `account_member` is the many-to-many membership: which normal accounts feed a
-- given summary. Both sides reference `account` and cascade, so deleting either a
-- summary or one of its members tidies the link automatically.

ALTER TABLE account
    ADD COLUMN kind TEXT NOT NULL DEFAULT 'normal'
        CHECK (kind IN ('normal', 'summary'));

CREATE TABLE account_member (
    summary_account_id INTEGER NOT NULL
        REFERENCES account (id) ON DELETE CASCADE,
    member_account_id  INTEGER NOT NULL
        REFERENCES account (id) ON DELETE CASCADE,
    PRIMARY KEY (summary_account_id, member_account_id)
) STRICT;

CREATE INDEX idx_account_member_summary ON account_member (summary_account_id);
CREATE INDEX idx_account_member_member  ON account_member (member_account_id);

//! Integration tests for the persistence layer, against a real (temp-file)
//! `SQLite` database created through the production [`crate::db::init_pool`] path
//! (so migrations, `foreign_keys=ON`, and WAL are all exercised).

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use sqlx::{AssertSqlSafe, SqlitePool};
use tempfile::TempDir;
use time::{Date, Month as TMonth};

use talea_core::{
    month_summary, Account, AccountId, AccountKind, AmountSegment, Category, CategoryIcon,
    CategoryId, Currency, Entry, EntryId, EntryKind, FreqUnit, Frequency, Money, Month,
    RecurringRule, RecurringRuleId, RuleEnd,
};

use crate::db;
use crate::dto::{IsoDate, NewBudget, NewEntry, NewRule, OccurrenceRef};
use crate::error::{CommandError, RepoError};
use crate::repo;

/// Inserts `draft` straight through the repository, in a transaction of its
/// own: no membership check, so tests can set up any summary they need.
async fn insert_account(pool: &SqlitePool, draft: &Account) -> Result<Account, RepoError> {
    let mut tx = pool.begin().await?;
    let account = repo::account::insert(&mut tx, draft).await?;
    tx.commit().await?;
    Ok(account)
}

/// Deletes account `id` straight through the repository, skipping the command's
/// membership check; `true` if it was there.
async fn delete_account(pool: &SqlitePool, id: AccountId) -> bool {
    let mut tx = pool.begin().await.unwrap();
    let deleted = repo::account::delete(&mut tx, id).await.unwrap();
    tx.commit().await.unwrap();
    deleted
}

/// A migrated temp-file database. The `TempDir` must be kept alive for the
/// lifetime of the pool.
async fn fixture() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let pool = db::init_pool(dir.path()).await.expect("init pool");
    (dir, pool)
}

fn date(y: i32, m: TMonth, d: u8) -> Date {
    Date::from_calendar_date(y, m, d).unwrap()
}

fn usd_account() -> Account {
    Account::new(
        AccountId::new(0),
        "Checking".to_owned(),
        "🏦".to_owned(),
        Currency::new("USD").unwrap(),
        Money::zero(),
        Month::new(2026, 1).unwrap(),
    )
    .unwrap()
}

async fn seed_account(pool: &SqlitePool) -> Account {
    insert_account(pool, &usd_account()).await.unwrap()
}

#[tokio::test]
async fn account_round_trips() {
    let (_dir, pool) = fixture().await;
    let draft = Account::new(
        AccountId::new(0),
        "Savings".to_owned(),
        "💰".to_owned(),
        Currency::new("eur").unwrap(),
        Money::from_minor_units(1234, 2), // 12.34
        Month::new(2026, 3).unwrap(),
    )
    .unwrap();

    let saved = insert_account(&pool, &draft).await.unwrap();
    assert_eq!(saved.currency().code(), "EUR");
    assert_eq!(saved.opening_balance(), Money::from_minor_units(1234, 2));

    let fetched = repo::account::get(&pool, saved.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched, saved);
    assert_eq!(repo::account::list(&pool).await.unwrap(), vec![saved]);
}

#[tokio::test]
async fn category_round_trips_both_icon_kinds() {
    let (_dir, pool) = fixture().await;
    let preset = repo::category::insert(
        &pool,
        &Category::new(
            CategoryId::new(0),
            "Rent".to_owned(),
            CategoryIcon::Preset("home".to_owned()),
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let emoji = repo::category::insert(
        &pool,
        &Category::new(
            CategoryId::new(0),
            "Food".to_owned(),
            CategoryIcon::Emoji("🍎".to_owned()),
        )
        .unwrap(),
    )
    .await
    .unwrap();

    let listed = repo::category::list(&pool).await.unwrap();
    assert_eq!(listed, vec![preset, emoji]);
}

#[tokio::test]
async fn entry_round_trips_with_money_and_date_fidelity() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let category = repo::category::insert(
        &pool,
        &Category::new(
            CategoryId::new(0),
            "Coffee".to_owned(),
            CategoryIcon::Emoji("☕".to_owned()),
        )
        .unwrap(),
    )
    .await
    .unwrap();

    let draft = Entry::new(
        EntryId::new(0),
        account.id(),
        Money::from_minor_units(10, 2), // 0.10 — exact decimal must survive
        EntryKind::Expense,
        date(2026, TMonth::January, 9),
        Some("tip".to_owned()),
        Some(category.id()),
    )
    .unwrap();

    let saved = repo::entry::insert(&pool, &draft).await.unwrap();
    let listed = repo::entry::for_account(&pool, account.id()).await.unwrap();
    assert_eq!(listed, vec![saved.clone()]);
    assert_eq!(saved.amount(), Money::from_minor_units(10, 2));
    assert_eq!(saved.date(), date(2026, TMonth::January, 9));
    assert_eq!(saved.note(), Some("tip"));
    assert_eq!(saved.category_id(), Some(category.id()));
}

#[tokio::test]
async fn rule_round_trips_never_and_until() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;

    let never = repo::rule::insert(
        &pool,
        &RecurringRule::new(
            RecurringRuleId::new(0),
            account.id(),
            Money::from_minor_units(200_000, 2),
            EntryKind::Income,
            Some("salary".to_owned()),
            None,
            date(2026, TMonth::January, 1),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        )
        .unwrap(),
    )
    .await
    .unwrap();

    let until = repo::rule::insert(
        &pool,
        &RecurringRule::new(
            RecurringRuleId::new(0),
            account.id(),
            Money::from_minor_units(1500, 2),
            EntryKind::Expense,
            None,
            None,
            date(2026, TMonth::January, 15),
            RuleEnd::Until(date(2026, TMonth::December, 31)),
            Frequency::new(FreqUnit::Weekly, 2).unwrap(),
        )
        .unwrap(),
    )
    .await
    .unwrap();

    let listed = repo::rule::for_account(&pool, account.id()).await.unwrap();
    assert_eq!(listed, vec![never, until]);
}

#[tokio::test]
async fn skip_and_detach_occurrence() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let rule = repo::rule::insert(
        &pool,
        &RecurringRule::new(
            RecurringRuleId::new(0),
            account.id(),
            Money::from_minor_units(5_000, 2),
            EntryKind::Expense,
            None,
            None,
            date(2026, TMonth::January, 5),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        )
        .unwrap(),
    )
    .await
    .unwrap();

    // Skip the February occurrence.
    repo::skip::add(&pool, rule.id(), date(2026, TMonth::February, 5))
        .await
        .unwrap();
    let skips = repo::skip::for_account(&pool, account.id()).await.unwrap();
    assert_eq!(skips, vec![(rule.id(), date(2026, TMonth::February, 5))]);

    // load_account_data attaches the skip, so February expands to nothing while
    // January (unaffected) still does.
    let rules = crate::commands::load_account_data(&pool, account.id())
        .await
        .unwrap()
        .rules;
    assert_eq!(rules[0].skips(), &[date(2026, TMonth::February, 5)]);
    assert_eq!(rules[0].expand_in(Month::new(2026, 2).unwrap()), []);
    assert_eq!(rules[0].expand_in(Month::new(2026, 1).unwrap()).len(), 1);

    // Detaching the March occurrence inserts a standalone entry and a skip.
    let draft = Entry::new(
        EntryId::new(0),
        account.id(),
        Money::from_minor_units(7_000, 2),
        EntryKind::Expense,
        date(2026, TMonth::March, 5),
        Some("higher this month".to_owned()),
        None,
    )
    .unwrap();
    let entry = repo::skip::detach(&pool, rule.id(), date(2026, TMonth::March, 5), &draft)
        .await
        .unwrap();
    assert!(entry.id().get() > 0);
    assert_eq!(
        repo::entry::for_account(&pool, account.id())
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repo::skip::for_account(&pool, account.id())
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn rule_amount_history_round_trips_through_breakpoints() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;

    let start = date(2026, TMonth::January, 15);
    // Base 1000 from Jan 15, raised to 1200 from Jun 1.
    let rule = RecurringRule::new_with_amounts(
        RecurringRuleId::new(0),
        account.id(),
        vec![
            AmountSegment::new(start, Money::from_minor_units(100_000, 2)),
            AmountSegment::new(
                date(2026, TMonth::June, 1),
                Money::from_minor_units(120_000, 2),
            ),
        ],
        EntryKind::Income,
        Some("salary".to_owned()),
        None,
        start,
        RuleEnd::Never,
        Frequency::new(FreqUnit::Monthly, 1).unwrap(),
    )
    .unwrap();
    let saved = repo::rule::insert(&pool, &rule).await.unwrap();

    let listed = repo::rule::for_account(&pool, account.id()).await.unwrap();
    assert_eq!(listed, vec![saved.clone()]);
    assert_eq!(listed[0].amounts().len(), 2);
    assert_eq!(
        listed[0].amount_on(date(2026, TMonth::June, 15)),
        Money::from_minor_units(120_000, 2)
    );

    // Updating to a single base amount drops the breakpoint.
    let collapsed = RecurringRule::new(
        saved.id(),
        account.id(),
        Money::from_minor_units(130_000, 2),
        EntryKind::Income,
        Some("salary".to_owned()),
        None,
        start,
        RuleEnd::Never,
        Frequency::new(FreqUnit::Monthly, 1).unwrap(),
    )
    .unwrap();
    assert!(repo::rule::update(&pool, &collapsed).await.unwrap());
    let relisted = repo::rule::for_account(&pool, account.id()).await.unwrap();
    assert_eq!(relisted, vec![collapsed]);
    assert_eq!(relisted[0].amounts().len(), 1);
}

#[tokio::test]
async fn delete_account_cascades_entries_and_rules() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    repo::entry::insert(
        &pool,
        &Entry::new(
            EntryId::new(0),
            account.id(),
            Money::from_minor_units(100, 2),
            EntryKind::Expense,
            date(2026, TMonth::January, 2),
            None,
            None,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    repo::rule::insert(
        &pool,
        &RecurringRule::new(
            RecurringRuleId::new(0),
            account.id(),
            Money::from_minor_units(100, 2),
            EntryKind::Income,
            None,
            None,
            date(2026, TMonth::January, 1),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        )
        .unwrap(),
    )
    .await
    .unwrap();

    assert!(delete_account(&pool, account.id()).await);
    // CASCADE only fires with foreign_keys=ON, so this also proves the pragma.
    assert_eq!(
        repo::entry::for_account(&pool, account.id()).await.unwrap(),
        []
    );
    assert_eq!(
        repo::rule::for_account(&pool, account.id()).await.unwrap(),
        []
    );
}

#[tokio::test]
async fn delete_category_nulls_entry_reference() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let category = repo::category::insert(
        &pool,
        &Category::new(
            CategoryId::new(0),
            "Misc".to_owned(),
            CategoryIcon::Preset("tag".to_owned()),
        )
        .unwrap(),
    )
    .await
    .unwrap();
    repo::entry::insert(
        &pool,
        &Entry::new(
            EntryId::new(0),
            account.id(),
            Money::from_minor_units(500, 2),
            EntryKind::Expense,
            date(2026, TMonth::January, 3),
            None,
            Some(category.id()),
        )
        .unwrap(),
    )
    .await
    .unwrap();

    assert!(repo::category::delete(&pool, category.id()).await.unwrap());
    let entries = repo::entry::for_account(&pool, account.id()).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].category_id(), None); // SET NULL, entry survives
}

#[tokio::test]
async fn ledger_query_matches_core_over_db() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await; // opening 0, anchor Jan 2026
    repo::entry::insert(
        &pool,
        &Entry::new(
            EntryId::new(0),
            account.id(),
            Money::from_minor_units(50_000, 2), // +500 income
            EntryKind::Income,
            date(2026, TMonth::January, 1),
            None,
            None,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    repo::rule::insert(
        &pool,
        &RecurringRule::new(
            RecurringRuleId::new(0),
            account.id(),
            Money::from_minor_units(20_000, 2), // -200 monthly expense
            EntryKind::Expense,
            None,
            None,
            date(2026, TMonth::January, 10),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        )
        .unwrap(),
    )
    .await
    .unwrap();

    // Replicates the `month_summary` command (repo load + pure core call).
    let entries = repo::entry::for_account(&pool, account.id()).await.unwrap();
    let rules = repo::rule::for_account(&pool, account.id()).await.unwrap();
    let jan = month_summary(
        Month::new(2026, 1).unwrap(),
        account.opening_balance(),
        account.anchor(),
        &entries,
        &rules,
        &[],
    );
    assert_eq!(jan.income, Money::from_minor_units(50_000, 2));
    assert_eq!(jan.expenses, Money::from_minor_units(20_000, 2));
    assert_eq!(jan.available, Money::from_minor_units(30_000, 2)); // 300

    let feb = month_summary(
        Month::new(2026, 2).unwrap(),
        account.opening_balance(),
        account.anchor(),
        &entries,
        &rules,
        &[],
    );
    assert_eq!(feb.carry_in, Money::from_minor_units(30_000, 2));
    assert_eq!(feb.available, Money::from_minor_units(10_000, 2)); // 300 - 200
}

#[tokio::test]
async fn reading_a_corrupt_row_is_an_error() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let account_key = i64::try_from(account.id().get()).unwrap();

    // Bypass the constructors with a raw insert of a zero amount (passes the
    // table CHECKs but the domain forbids non-positive amounts). The read path's
    // constructor must reject it as corruption.
    sqlx::query("INSERT INTO entry (account_id, amount, kind, date) VALUES (?, ?, ?, ?)")
        .bind(account_key)
        .bind("0")
        .bind("expense")
        .bind("2026-01-04")
        .execute(&pool)
        .await
        .unwrap();

    let result = repo::entry::for_account(&pool, account.id()).await;
    assert!(matches!(result, Err(crate::error::RepoError::Corrupt(_))));
}

#[tokio::test]
async fn create_account_from_frontend_shaped_json() {
    // Mirrors exactly what the frontend sends for `create_account`'s `account`
    // argument; confirms the NewAccount payload deserializes, validates, and
    // persists (so an "unexpected error" in the UI is a client-side input
    // problem, not this path).
    let (_dir, pool) = fixture().await;
    let json = r#"{
        "name": "Personal",
        "icon": "💰",
        "currency": "USD",
        "opening_balance": "0.00",
        "anchor": { "year": 2026, "month": 5 }
    }"#;
    let draft = serde_json::from_str::<crate::dto::NewAccount>(json)
        .expect("NewAccount deserializes")
        .build()
        .expect("draft validates");
    let saved = insert_account(&pool, &draft).await.unwrap();
    assert_eq!(saved.currency().code(), "USD");
    assert_eq!(saved.name(), "Personal");
}

#[tokio::test]
async fn migrations_are_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let _first = db::init_pool(dir.path()).await.unwrap();
    // Re-opening the same database re-runs the migrator, which must no-op.
    let _second = db::init_pool(dir.path()).await.unwrap();
}

#[tokio::test]
async fn load_account_data_loads_or_reports_not_found() {
    let (_dir, pool) = fixture().await;

    // Missing account -> NotFound.
    let missing = crate::commands::load_account_data(&pool, AccountId::new(9999)).await;
    assert!(matches!(missing, Err(CommandError::NotFound)));

    // Existing account -> its account + entries + rules, in one snapshot.
    let account = seed_account(&pool).await;
    repo::entry::insert(
        &pool,
        &Entry::new(
            EntryId::new(0),
            account.id(),
            Money::from_minor_units(100, 2),
            EntryKind::Expense,
            date(2026, TMonth::January, 2),
            None,
            None,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    repo::rule::insert(
        &pool,
        &RecurringRule::new(
            RecurringRuleId::new(0),
            account.id(),
            Money::from_minor_units(100, 2),
            EntryKind::Income,
            None,
            None,
            date(2026, TMonth::January, 1),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        )
        .unwrap(),
    )
    .await
    .unwrap();

    let data = crate::commands::load_account_data(&pool, account.id())
        .await
        .unwrap();
    assert_eq!(data.account.id(), account.id());
    assert_eq!(data.entries.len(), 1);
    assert_eq!(data.rules.len(), 1);
}

#[test]
fn command_error_maps_and_serializes() {
    // Domain validation failures (user input) become Validation.
    let from_domain: CommandError = talea_core::DomainError::MonthOutOfRange(13).into();
    assert!(matches!(from_domain, CommandError::Validation(_)));

    // Repo errors map to the right boundary codes.
    assert!(matches!(
        CommandError::from(RepoError::InvalidId(42)),
        CommandError::Validation(_)
    ));
    assert!(matches!(
        CommandError::from(RepoError::Corrupt("bad".to_owned())),
        CommandError::Corrupt
    ));
    assert!(matches!(
        CommandError::from(RepoError::Sqlx(sqlx::Error::RowNotFound)),
        CommandError::Database
    ));
    assert!(matches!(
        CommandError::from(RepoError::Io(std::io::Error::other("disk"))),
        CommandError::Database
    ));

    // Serializes as { code, message }; internal detail is not leaked.
    let value: serde_json::Value = serde_json::from_str(
        &serde_json::to_string(&CommandError::Validation("nope".to_owned())).unwrap(),
    )
    .unwrap();
    assert_eq!(value["code"], "validation");
    assert_eq!(value["message"], "nope");

    let db_json = serde_json::to_string(&CommandError::Database).unwrap();
    assert!(db_json.contains(r#""code":"database""#));
    assert!(!db_json.to_lowercase().contains("sql"));
}

#[tokio::test]
async fn entry_update_persists_changes() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let saved = repo::entry::insert(
        &pool,
        &Entry::new(
            EntryId::new(0),
            account.id(),
            Money::from_minor_units(500, 2),
            EntryKind::Expense,
            date(2026, TMonth::January, 5),
            None,
            None,
        )
        .unwrap(),
    )
    .await
    .unwrap();

    let updated = Entry::new(
        saved.id(),
        account.id(),
        Money::from_minor_units(750, 2),
        EntryKind::Income,
        date(2026, TMonth::January, 6),
        Some("revised".to_owned()),
        None,
    )
    .unwrap();
    assert!(repo::entry::update(&pool, &updated).await.unwrap());
    assert_eq!(
        repo::entry::for_account(&pool, account.id()).await.unwrap(),
        vec![updated]
    );
}

#[tokio::test]
async fn rule_update_persists_changes() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let saved = repo::rule::insert(
        &pool,
        &RecurringRule::new(
            RecurringRuleId::new(0),
            account.id(),
            Money::from_minor_units(1000, 2),
            EntryKind::Expense,
            None,
            None,
            date(2026, TMonth::January, 1),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        )
        .unwrap(),
    )
    .await
    .unwrap();

    let updated = RecurringRule::new(
        saved.id(),
        account.id(),
        Money::from_minor_units(2000, 2),
        EntryKind::Income,
        Some("raise".to_owned()),
        None,
        date(2026, TMonth::January, 1),
        RuleEnd::Until(date(2027, TMonth::January, 1)),
        Frequency::new(FreqUnit::Weekly, 2).unwrap(),
    )
    .unwrap();
    assert!(repo::rule::update(&pool, &updated).await.unwrap());
    assert_eq!(
        repo::rule::for_account(&pool, account.id()).await.unwrap(),
        vec![updated]
    );
}

fn account_with(currency: &str, name: &str) -> Account {
    Account::new(
        AccountId::new(0),
        name.to_owned(),
        "💰".to_owned(),
        Currency::new(currency).unwrap(),
        Money::zero(),
        Month::new(2026, 1).unwrap(),
    )
    .unwrap()
}

fn new_entry(account_id: AccountId, minor: i64, kind: EntryKind) -> NewEntry {
    NewEntry {
        account_id,
        amount: Money::from_minor_units(minor, 2),
        kind,
        date: date(2026, TMonth::March, 3),
        note: Some("move".to_owned()),
        category_id: None,
    }
}

#[tokio::test]
async fn transfer_mirrors_entry_onto_the_other_account() {
    let (_dir, pool) = fixture().await;
    let from = seed_account(&pool).await; // USD
    let to = insert_account(&pool, &account_with("USD", "Savings"))
        .await
        .unwrap();

    let (primary, counter) = crate::commands::transfer(
        &pool,
        new_entry(from.id(), 5_000, EntryKind::Expense),
        to.id(),
    )
    .await
    .unwrap();

    // The counterpart mirrors the entry on the other account with the opposite kind.
    assert_eq!(primary.account_id(), from.id());
    assert_eq!(primary.kind(), EntryKind::Expense);
    assert_eq!(counter.account_id(), to.id());
    assert_eq!(counter.kind(), EntryKind::Income);
    assert_eq!(counter.amount(), primary.amount());
    assert_eq!(counter.date(), primary.date());
    // One entry persisted per account.
    assert_eq!(
        repo::entry::for_account(&pool, from.id())
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repo::entry::for_account(&pool, to.id())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn transfer_rejects_bad_pairings() {
    let (_dir, pool) = fixture().await;
    let usd = seed_account(&pool).await;
    let eur = insert_account(&pool, &account_with("EUR", "Euro"))
        .await
        .unwrap();

    // Different currencies (no conversion).
    assert!(matches!(
        crate::commands::transfer(
            &pool,
            new_entry(usd.id(), 1_000, EntryKind::Expense),
            eur.id()
        )
        .await,
        Err(CommandError::Validation(_))
    ));
    // Same account on both sides.
    assert!(matches!(
        crate::commands::transfer(
            &pool,
            new_entry(usd.id(), 1_000, EntryKind::Expense),
            usd.id()
        )
        .await,
        Err(CommandError::Validation(_))
    ));
    // Missing counterpart account.
    assert!(matches!(
        crate::commands::transfer(
            &pool,
            new_entry(usd.id(), 1_000, EntryKind::Expense),
            AccountId::new(9999)
        )
        .await,
        Err(CommandError::NotFound)
    ));
}

#[tokio::test]
async fn load_account_rules_returns_rules_or_not_found() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    repo::rule::insert(
        &pool,
        &RecurringRule::new(
            RecurringRuleId::new(0),
            account.id(),
            Money::from_minor_units(1_000, 2),
            EntryKind::Income,
            None,
            None,
            date(2026, TMonth::January, 1),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        )
        .unwrap(),
    )
    .await
    .unwrap();

    let rules = crate::commands::load_account_rules(&pool, account.id())
        .await
        .unwrap();
    assert_eq!(rules.len(), 1);

    assert!(matches!(
        crate::commands::load_account_rules(&pool, AccountId::new(9999)).await,
        Err(CommandError::NotFound)
    ));
}

#[tokio::test]
async fn occurrence_commands_enforce_ownership_and_apply() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let other = insert_account(&pool, &account_with("USD", "Other"))
        .await
        .unwrap();
    let rule = repo::rule::insert(
        &pool,
        &RecurringRule::new(
            RecurringRuleId::new(0),
            account.id(),
            Money::from_minor_units(5_000, 2),
            EntryKind::Expense,
            None,
            None,
            date(2026, TMonth::January, 5),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let feb = || OccurrenceRef {
        rule_id: rule.id(),
        date: date(2026, TMonth::February, 5),
    };
    let mar = || OccurrenceRef {
        rule_id: rule.id(),
        date: date(2026, TMonth::March, 5),
    };

    // A rule not owned by the given account is rejected, with no skip written.
    assert!(matches!(
        crate::commands::skip_occurrence_inner(&pool, other.id(), feb()).await,
        Err(CommandError::NotFound)
    ));
    assert_eq!(
        repo::skip::for_account(&pool, account.id()).await.unwrap(),
        []
    );

    // The owning account can skip the occurrence.
    crate::commands::skip_occurrence_inner(&pool, account.id(), feb())
        .await
        .unwrap();
    assert_eq!(
        repo::skip::for_account(&pool, account.id())
            .await
            .unwrap()
            .len(),
        1
    );

    // Detach is likewise ownership-scoped; the owning account materializes a
    // standalone entry plus a skip for that occurrence.
    assert!(matches!(
        crate::commands::detach_occurrence_inner(
            &pool,
            other.id(),
            mar(),
            new_entry(account.id(), 7_000, EntryKind::Expense)
        )
        .await,
        Err(CommandError::NotFound)
    ));
    let entry = crate::commands::detach_occurrence_inner(
        &pool,
        account.id(),
        mar(),
        new_entry(account.id(), 7_000, EntryKind::Expense),
    )
    .await
    .unwrap();
    assert!(entry.id().get() > 0);
    assert_eq!(
        repo::entry::for_account(&pool, account.id())
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repo::skip::for_account(&pool, account.id())
            .await
            .unwrap()
            .len(),
        2
    );
}

// ---- Backup / restore -------------------------------------------------------

#[tokio::test]
async fn backup_snapshot_restores_over_existing_data() {
    // Source DB with one account.
    let (src_dir, src) = fixture().await;
    seed_account(&src).await; // "Checking"
    let bytes = crate::backup::snapshot(&src, src_dir.path()).await.unwrap();
    assert!(bytes.starts_with(b"SQLite format 3\0"));

    // Destination DB seeded with a *different* account, to prove it's replaced.
    let (dst_dir, dst) = fixture().await;
    let other = Account::new(
        AccountId::new(0),
        "Wallet".to_owned(),
        "👛".to_owned(),
        Currency::new("USD").unwrap(),
        Money::zero(),
        Month::new(2026, 1).unwrap(),
    )
    .unwrap();
    insert_account(&dst, &other).await.unwrap();

    crate::backup::restore(&dst, dst_dir.path(), &bytes)
        .await
        .unwrap();

    // The destination now mirrors the source exactly.
    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM account ORDER BY name")
        .fetch_all(&dst)
        .await
        .unwrap();
    assert_eq!(names, vec!["Checking".to_owned()]);
}

// ---- Summary accounts -------------------------------------------------------

fn summary_over(currency: &str, name: &str, members: Vec<AccountId>) -> Account {
    Account::new_summary(
        AccountId::new(0),
        name.to_owned(),
        "📊".to_owned(),
        Currency::new(currency).unwrap(),
        Month::new(2026, 1).unwrap(),
        members,
    )
    .unwrap()
}

/// Inserts an entry built from the `new_entry` helper (March 2026, given amount).
async fn seed_entry(pool: &SqlitePool, account_id: AccountId, minor: i64, kind: EntryKind) {
    let draft = new_entry(account_id, minor, kind).build().unwrap();
    repo::entry::insert(pool, &draft).await.unwrap();
}

#[tokio::test]
async fn summary_account_combines_member_month_summaries() {
    let (_dir, pool) = fixture().await;
    let a = insert_account(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let b = insert_account(&pool, &account_with("USD", "B"))
        .await
        .unwrap();
    seed_entry(&pool, a.id(), 50_000, EntryKind::Income).await; // +500 on A
    seed_entry(&pool, b.id(), 30_000, EntryKind::Income).await; // +300 on B
    seed_entry(&pool, b.id(), 10_000, EntryKind::Expense).await; // -100 on B

    let summary = insert_account(&pool, &summary_over("USD", "All", vec![a.id(), b.id()]))
        .await
        .unwrap();
    assert_eq!(summary.kind(), AccountKind::Summary);
    assert_eq!(summary.members(), &[a.id(), b.id()]);

    let march = Month::new(2026, 3).unwrap();
    let combined = crate::commands::month_summary_inner(&pool, summary.id(), march)
        .await
        .unwrap();
    assert_eq!(combined.income, Money::from_minor_units(80_000, 2)); // 800.00
    assert_eq!(combined.expenses, Money::from_minor_units(10_000, 2)); // 100.00
    assert_eq!(combined.available, Money::from_minor_units(70_000, 2)); // 700.00
}

#[tokio::test]
async fn summary_account_merges_member_category_expenses() {
    let (_dir, pool) = fixture().await;
    let a = insert_account(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let b = insert_account(&pool, &account_with("USD", "B"))
        .await
        .unwrap();
    seed_entry(&pool, a.id(), 10_000, EntryKind::Expense).await; // -100 (uncategorized)
    seed_entry(&pool, b.id(), 20_000, EntryKind::Expense).await; // -200 (uncategorized)

    let summary = insert_account(&pool, &summary_over("USD", "All", vec![a.id(), b.id()]))
        .await
        .unwrap();
    let march = Month::new(2026, 3).unwrap();
    let breakdown = crate::commands::expenses_by_category_inner(&pool, summary.id(), march)
        .await
        .unwrap();
    assert_eq!(breakdown.len(), 1); // both fold into the uncategorized "Other" bucket
    assert_eq!(breakdown[0].category_id, None);
    assert_eq!(breakdown[0].total, Money::from_minor_units(30_000, 2)); // 300.00
}

#[tokio::test]
async fn writes_to_a_summary_account_are_rejected() {
    let (_dir, pool) = fixture().await;
    let a = insert_account(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let summary = insert_account(&pool, &summary_over("USD", "All", vec![a.id()]))
        .await
        .unwrap();
    assert!(matches!(
        crate::commands::reject_if_summary(&pool, summary.id()).await,
        Err(CommandError::Validation(_))
    ));
    // A normal account is fine.
    crate::commands::reject_if_summary(&pool, a.id())
        .await
        .unwrap();
}

// ---- Account commands check and write in one step -----------------------------

/// A summary account over `members`, as the frontend sends one.
fn new_summary(currency: &str, members: Vec<AccountId>) -> crate::dto::NewAccount {
    crate::dto::NewAccount {
        name: "All".to_owned(),
        icon: "📊".to_owned(),
        currency: Currency::new(currency).unwrap(),
        opening_balance: Money::zero(),
        anchor: Month::new(2026, 1).unwrap(),
        kind: AccountKind::Summary,
        members,
    }
}

/// `account` in another currency, otherwise unchanged.
fn in_currency(account: &Account, currency: &str) -> Account {
    Account::new(
        account.id(),
        account.name().to_owned(),
        account.icon().to_owned(),
        Currency::new(currency).unwrap(),
        account.opening_balance(),
        account.anchor(),
    )
    .unwrap()
}

#[tokio::test]
async fn a_summary_is_created_only_over_members_of_its_currency() {
    let (_dir, pool) = fixture().await;
    let usd = insert_account(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let refused =
        crate::commands::create_account_inner(&pool, new_summary("EUR", vec![usd.id()])).await;
    assert!(
        matches!(refused, Err(CommandError::Validation(_))),
        "{refused:?}"
    );
    assert_eq!(repo::account::list(&pool).await.unwrap().len(), 1);
    let summary = crate::commands::create_account_inner(&pool, new_summary("USD", vec![usd.id()]))
        .await
        .unwrap();
    assert_eq!(summary.members(), [usd.id()]);
}

#[tokio::test]
async fn a_member_keeps_its_currency_while_it_belongs_to_a_summary() {
    let (_dir, pool) = fixture().await;
    let member = insert_account(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let lone = insert_account(&pool, &account_with("USD", "B"))
        .await
        .unwrap();
    crate::commands::create_account_inner(&pool, new_summary("USD", vec![member.id()]))
        .await
        .unwrap();
    let refused = crate::commands::update_account_inner(&pool, in_currency(&member, "EUR")).await;
    assert!(
        matches!(refused, Err(CommandError::Validation(_))),
        "{refused:?}"
    );
    let stored = repo::account::get(&pool, member.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.currency().code(), "USD");
    let moved = crate::commands::update_account_inner(&pool, in_currency(&lone, "EUR"))
        .await
        .unwrap();
    assert_eq!(moved.currency().code(), "EUR");
    let missing = Account::new(
        AccountId::new(999),
        "Gone".to_owned(),
        "💰".to_owned(),
        Currency::new("USD").unwrap(),
        Money::zero(),
        Month::new(2026, 1).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        crate::commands::update_account_inner(&pool, missing).await,
        Err(CommandError::NotFound)
    ));
}

#[tokio::test]
async fn an_account_is_deleted_only_once_no_summary_holds_it() {
    let (_dir, pool) = fixture().await;
    let member = insert_account(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let summary =
        crate::commands::create_account_inner(&pool, new_summary("USD", vec![member.id()]))
            .await
            .unwrap();
    let refused = crate::commands::delete_account_inner(&pool, member.id()).await;
    assert!(
        matches!(refused, Err(CommandError::Validation(_))),
        "{refused:?}"
    );
    assert!(repo::account::get(&pool, member.id())
        .await
        .unwrap()
        .is_some());
    // A summary itself may always go, and with it the membership.
    crate::commands::delete_account_inner(&pool, summary.id())
        .await
        .unwrap();
    crate::commands::delete_account_inner(&pool, member.id())
        .await
        .unwrap();
    assert_eq!(
        repo::account::list(&pool).await.unwrap(),
        Vec::<Account>::new()
    );
    assert!(matches!(
        crate::commands::delete_account_inner(&pool, member.id()).await,
        Err(CommandError::NotFound)
    ));
}

/// A second connection to the fixture's database that gives up at once when
/// the database is locked, rather than waiting.
async fn impatient_connection(dir: &TempDir) -> sqlx::SqliteConnection {
    <sqlx::SqliteConnection as sqlx::Connection>::connect_with(
        &SqliteConnectOptions::new()
            .filename(dir.path().join(crate::db::DB_FILENAME))
            .busy_timeout(std::time::Duration::ZERO),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn a_write_transaction_holds_the_lock_from_its_start() {
    // `BEGIN IMMEDIATE` takes the lock as the transaction opens, so no other
    // write can land between what a command checks and what it writes; a
    // plain `BEGIN` would take it only at the first write.
    let (dir, pool) = fixture().await;
    let mut other = impatient_connection(&dir).await;
    let held = crate::db::begin_write(&pool).await.unwrap();
    assert!(sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut other)
        .await
        .is_err());
    held.rollback().await.unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut other)
        .await
        .unwrap();
    sqlx::query("ROLLBACK").execute(&mut other).await.unwrap();
    sqlx::Connection::close(other).await.unwrap();
}

/// A database holding account A in a USD summary, and a second connection to
/// it, for a test of whether a command checks under the write lock.
async fn member_of_a_summary() -> (TempDir, SqlitePool, Account, sqlx::SqliteConnection) {
    let (dir, pool) = fixture().await;
    let member = insert_account(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    crate::commands::create_account_inner(&pool, new_summary("USD", vec![member.id()]))
        .await
        .unwrap();
    let other = impatient_connection(&dir).await;
    (dir, pool, member, other)
}

/// Runs `refused` (a command its check refuses) once while `other` holds the
/// write lock, then once it doesn't. A command that checked in a plain
/// transaction would read and refuse at once; one that takes the lock first
/// waits for it, and refuses only once it's free. It must then leave the lock
/// free. Each test runs one command on a fresh pool, so a command that times
/// out can't leave the next one waiting on the pool instead of the lock.
async fn checks_under_the_write_lock<F, Fut>(
    pool: SqlitePool,
    mut other: sqlx::SqliteConnection,
    refused: F,
) where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<(), CommandError>>,
{
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut other)
        .await
        .unwrap();
    let wait = std::time::Duration::from_millis(500);
    assert!(tokio::time::timeout(wait, refused()).await.is_err());
    sqlx::query("ROLLBACK").execute(&mut other).await.unwrap();
    assert!(matches!(refused().await, Err(CommandError::Validation(_))));
    // The timed-out command's connection takes the lock once more as it
    // rolls back; closing the pool waits until it has.
    pool.close().await;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut other)
        .await
        .unwrap();
    sqlx::query("ROLLBACK").execute(&mut other).await.unwrap();
    sqlx::Connection::close(other).await.unwrap();
}

#[tokio::test]
async fn creating_a_summary_checks_under_the_write_lock() {
    let (_dir, pool, member, other) = member_of_a_summary().await;
    let refused = || async {
        crate::commands::create_account_inner(&pool, new_summary("EUR", vec![member.id()]))
            .await
            .map(drop)
    };
    checks_under_the_write_lock(pool.clone(), other, refused).await;
}

#[tokio::test]
async fn updating_an_account_checks_under_the_write_lock() {
    let (_dir, pool, member, other) = member_of_a_summary().await;
    let refused = || async {
        crate::commands::update_account_inner(&pool, in_currency(&member, "EUR"))
            .await
            .map(drop)
    };
    checks_under_the_write_lock(pool.clone(), other, refused).await;
}

#[tokio::test]
async fn deleting_an_account_checks_under_the_write_lock() {
    let (_dir, pool, member, other) = member_of_a_summary().await;
    let refused = || crate::commands::delete_account_inner(&pool, member.id());
    checks_under_the_write_lock(pool.clone(), other, refused).await;
}

#[tokio::test]
async fn a_transfer_checks_under_the_write_lock() {
    let (_dir, pool, member, other) = member_of_a_summary().await;
    let euro = insert_account(&pool, &account_with("EUR", "Euro"))
        .await
        .unwrap();
    let refused = || async {
        crate::commands::transfer(
            &pool,
            new_entry(member.id(), 1_00, EntryKind::Expense),
            euro.id(),
        )
        .await
        .map(drop)
    };
    checks_under_the_write_lock(pool.clone(), other, refused).await;
}

/// The summary account [`member_of_a_summary`] made.
async fn the_summary(pool: &SqlitePool) -> Account {
    repo::account::list(pool)
        .await
        .unwrap()
        .into_iter()
        .find(|acc| acc.kind() == AccountKind::Summary)
        .unwrap()
}

#[tokio::test]
async fn creating_an_entry_checks_under_the_write_lock() {
    let (_dir, pool, _member, other) = member_of_a_summary().await;
    let summary = the_summary(&pool).await;
    let refused = || async {
        crate::commands::create_entry_inner(
            &pool,
            new_entry(summary.id(), 1_00, EntryKind::Expense),
        )
        .await
        .map(drop)
    };
    checks_under_the_write_lock(pool.clone(), other, refused).await;
}

#[tokio::test]
async fn creating_a_budget_checks_under_the_write_lock() {
    let (_dir, pool, _member, other) = member_of_a_summary().await;
    let summary = the_summary(&pool).await;
    let category = seed_category(&pool, "Food").await;
    let refused = || async {
        crate::commands::create_budget_inner(&pool, new_budget(summary.id(), category.id(), 1_00))
            .await
            .map(drop)
    };
    checks_under_the_write_lock(pool.clone(), other, refused).await;
}

#[tokio::test]
async fn skipping_an_occurrence_checks_under_the_write_lock() {
    let (_dir, pool, _member, other) = member_of_a_summary().await;
    let summary = the_summary(&pool).await;
    // Refused for the summary before the rule is looked up, so none is needed.
    let occurrence = || OccurrenceRef {
        rule_id: RecurringRuleId::new(1),
        date: date(2026, TMonth::February, 5),
    };
    let refused = || crate::commands::skip_occurrence_inner(&pool, summary.id(), occurrence());
    checks_under_the_write_lock(pool.clone(), other, refused).await;
}

#[tokio::test]
async fn detaching_an_occurrence_checks_under_the_write_lock() {
    let (_dir, pool, _member, other) = member_of_a_summary().await;
    let summary = the_summary(&pool).await;
    let occurrence = || OccurrenceRef {
        rule_id: RecurringRuleId::new(1),
        date: date(2026, TMonth::February, 5),
    };
    let refused = || async {
        crate::commands::detach_occurrence_inner(
            &pool,
            summary.id(),
            occurrence(),
            new_entry(summary.id(), 1_00, EntryKind::Expense),
        )
        .await
        .map(drop)
    };
    checks_under_the_write_lock(pool.clone(), other, refused).await;
}

#[tokio::test]
async fn updating_an_entry_checks_under_the_write_lock() {
    let (_dir, pool, _member, other) = member_of_a_summary().await;
    let summary = the_summary(&pool).await;
    // Refused for the summary before the row is looked up, so none is needed.
    let entry = new_entry(summary.id(), 1_00, EntryKind::Expense)
        .build()
        .unwrap();
    let refused = || async {
        crate::commands::update_entry_inner(&pool, entry.clone())
            .await
            .map(drop)
    };
    checks_under_the_write_lock(pool.clone(), other, refused).await;
}

/// A monthly expense rule on `account_id`, as the frontend sends one.
fn new_rule(account_id: AccountId) -> NewRule {
    NewRule {
        account_id,
        amount: Money::from_minor_units(1_00, 2),
        kind: EntryKind::Expense,
        note: None,
        category_id: None,
        start_date: date(2026, TMonth::January, 5),
        end: RuleEnd::Never,
        frequency: Frequency::new(FreqUnit::Monthly, 1).unwrap(),
    }
}

#[tokio::test]
async fn creating_a_rule_checks_under_the_write_lock() {
    let (_dir, pool, _member, other) = member_of_a_summary().await;
    let summary = the_summary(&pool).await;
    let refused = || async {
        crate::commands::create_rule_inner(&pool, new_rule(summary.id()))
            .await
            .map(drop)
    };
    checks_under_the_write_lock(pool.clone(), other, refused).await;
}

#[tokio::test]
async fn updating_a_rule_checks_under_the_write_lock() {
    let (_dir, pool, _member, other) = member_of_a_summary().await;
    let summary = the_summary(&pool).await;
    let rule = new_rule(summary.id()).build().unwrap();
    let refused = || async {
        crate::commands::update_rule_inner(&pool, rule.clone())
            .await
            .map(drop)
    };
    checks_under_the_write_lock(pool.clone(), other, refused).await;
}

#[tokio::test]
async fn updating_a_budget_checks_under_the_write_lock() {
    let (_dir, pool, _member, other) = member_of_a_summary().await;
    let summary = the_summary(&pool).await;
    let category = seed_category(&pool, "Food").await;
    let budget = new_budget(summary.id(), category.id(), 1_00)
        .build()
        .unwrap();
    let refused = || async {
        crate::commands::update_budget_inner(&pool, budget.clone())
            .await
            .map(drop)
    };
    checks_under_the_write_lock(pool.clone(), other, refused).await;
}

#[tokio::test]
async fn summary_membership_requires_matching_currency() {
    let (_dir, pool) = fixture().await;
    let usd = insert_account(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    // A EUR summary cannot take a USD member...
    let mut tx = pool.begin().await.unwrap();
    let draft = summary_over("EUR", "All", vec![usd.id()]);
    assert!(matches!(
        crate::commands::validate_membership(&mut tx, &draft).await,
        Err(CommandError::Validation(_))
    ));
    // ...while a USD one can.
    let draft = summary_over("USD", "All", vec![usd.id()]);
    assert!(crate::commands::validate_membership(&mut tx, &draft)
        .await
        .is_ok());
}

#[tokio::test]
async fn backup_round_trip_preserves_summary_membership() {
    let (src_dir, src) = fixture().await;
    let a = insert_account(&src, &account_with("USD", "A"))
        .await
        .unwrap();
    let b = insert_account(&src, &account_with("USD", "B"))
        .await
        .unwrap();
    insert_account(&src, &summary_over("USD", "All", vec![a.id(), b.id()]))
        .await
        .unwrap();
    let bytes = crate::backup::snapshot(&src, src_dir.path()).await.unwrap();

    let (dst_dir, dst) = fixture().await;
    crate::backup::restore(&dst, dst_dir.path(), &bytes)
        .await
        .unwrap();
    let restored = repo::account::list(&dst).await.unwrap();
    let summary = restored
        .iter()
        .find(|acc| acc.kind() == AccountKind::Summary)
        .expect("summary account survived the restore");
    assert_eq!(summary.members().len(), 2);
}

// ---- Category budgets -------------------------------------------------------

async fn seed_category(pool: &SqlitePool, label: &str) -> Category {
    let draft = Category::new(
        CategoryId::new(0),
        label.to_owned(),
        CategoryIcon::Preset("tag".to_owned()),
    )
    .unwrap();
    repo::category::insert(pool, &draft).await.unwrap()
}

fn new_budget(account_id: AccountId, category_id: CategoryId, minor: i64) -> NewBudget {
    NewBudget {
        account_id,
        category_id,
        amount: Money::from_minor_units(minor, 2),
        start: Month::new(2026, 1).unwrap(),
        end: None,
    }
}

/// Inserts a March 2026 expense of `minor` classified under `category_id`.
async fn seed_categorized_expense(
    pool: &SqlitePool,
    account_id: AccountId,
    category_id: CategoryId,
    minor: i64,
) {
    let mut draft = new_entry(account_id, minor, EntryKind::Expense);
    draft.category_id = Some(category_id);
    repo::entry::insert(pool, &draft.build().unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn budget_round_trips_with_and_without_an_end() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let groceries = seed_category(&pool, "Groceries").await;
    let rent = seed_category(&pool, "Rent").await;

    let open = crate::commands::create_budget_inner(
        &pool,
        new_budget(account.id(), groceries.id(), 40_000),
    )
    .await
    .unwrap();
    assert_eq!(open.base_amount(), Money::from_minor_units(40_000, 2));
    assert_eq!(open.end(), None);

    let mut bounded_draft = new_budget(account.id(), rent.id(), 95_000);
    bounded_draft.end = Some(Month::new(2026, 6).unwrap());
    let bounded = crate::commands::create_budget_inner(&pool, bounded_draft)
        .await
        .unwrap();
    assert_eq!(bounded.end(), Some(Month::new(2026, 6).unwrap()));

    let loaded = repo::budget::for_account(&pool, account.id())
        .await
        .unwrap();
    assert_eq!(loaded, vec![open, bounded.clone()]);

    // Update round-trips the changed amount and clears the end month.
    let edited = talea_core::Budget::new(
        bounded.id(),
        bounded.account_id(),
        bounded.category_id(),
        Money::from_minor_units(99_000, 2),
        bounded.start(),
        None,
    )
    .unwrap();
    crate::commands::update_budget_inner(&pool, edited.clone())
        .await
        .unwrap();
    let loaded = repo::budget::for_account(&pool, account.id())
        .await
        .unwrap();
    assert_eq!(loaded[1], edited);

    assert!(repo::budget::delete(&pool, edited.id()).await.unwrap());
    assert_eq!(
        repo::budget::for_account(&pool, account.id())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn delete_category_cascades_its_budgets() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let groceries = seed_category(&pool, "Groceries").await;
    crate::commands::create_budget_inner(&pool, new_budget(account.id(), groceries.id(), 40_000))
        .await
        .unwrap();

    // Unlike entries (SET NULL), a budget cannot outlive its category.
    assert!(repo::category::delete(&pool, groceries.id()).await.unwrap());
    assert_eq!(
        repo::budget::for_account(&pool, account.id())
            .await
            .unwrap(),
        []
    );
}

#[tokio::test]
async fn delete_account_cascades_its_budgets() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let groceries = seed_category(&pool, "Groceries").await;
    crate::commands::create_budget_inner(&pool, new_budget(account.id(), groceries.id(), 40_000))
        .await
        .unwrap();

    assert!(delete_account(&pool, account.id()).await);
    assert_eq!(
        repo::budget::for_account(&pool, account.id())
            .await
            .unwrap(),
        []
    );
}

#[tokio::test]
async fn duplicate_budget_per_category_is_rejected() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let groceries = seed_category(&pool, "Groceries").await;
    let first = crate::commands::create_budget_inner(
        &pool,
        new_budget(account.id(), groceries.id(), 40_000),
    )
    .await
    .unwrap();

    assert!(matches!(
        crate::commands::create_budget_inner(
            &pool,
            new_budget(account.id(), groceries.id(), 10_000)
        )
        .await,
        Err(CommandError::Validation(_))
    ));

    // Updating the existing budget must not collide with itself.
    crate::commands::update_budget_inner(&pool, first)
        .await
        .unwrap();
}

#[tokio::test]
async fn budget_on_a_summary_account_is_rejected() {
    let (_dir, pool) = fixture().await;
    let a = insert_account(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let summary = insert_account(&pool, &summary_over("USD", "All", vec![a.id()]))
        .await
        .unwrap();
    let groceries = seed_category(&pool, "Groceries").await;

    assert!(matches!(
        crate::commands::create_budget_inner(
            &pool,
            new_budget(summary.id(), groceries.id(), 40_000)
        )
        .await,
        Err(CommandError::Validation(_))
    ));

    // Nor may a crafted update move an existing budget onto the summary.
    let budget =
        crate::commands::create_budget_inner(&pool, new_budget(a.id(), groceries.id(), 40_000))
            .await
            .unwrap();
    let moved = talea_core::Budget::new(
        budget.id(),
        summary.id(),
        budget.category_id(),
        budget.base_amount(),
        budget.start(),
        None,
    )
    .unwrap();
    assert!(matches!(
        crate::commands::update_budget_inner(&pool, moved).await,
        Err(CommandError::Validation(_))
    ));
}

#[tokio::test]
async fn budget_for_a_missing_category_is_rejected() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    assert!(matches!(
        crate::commands::create_budget_inner(
            &pool,
            new_budget(account.id(), CategoryId::new(999), 40_000)
        )
        .await,
        Err(CommandError::Validation(_))
    ));
}

#[tokio::test]
async fn month_summary_reserves_unspent_budget() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let groceries = seed_category(&pool, "Groceries").await;
    seed_entry(&pool, account.id(), 200_000, EntryKind::Income).await; // +2000
    let march = Month::new(2026, 3).unwrap();

    let before = crate::commands::month_summary_inner(&pool, account.id(), march)
        .await
        .unwrap();
    assert_eq!(before.reserved, Money::zero());
    assert_eq!(before.free, before.available);

    crate::commands::create_budget_inner(&pool, new_budget(account.id(), groceries.id(), 40_000))
        .await
        .unwrap();
    seed_categorized_expense(&pool, account.id(), groceries.id(), 15_000).await; // -150

    let after = crate::commands::month_summary_inner(&pool, account.id(), march)
        .await
        .unwrap();
    // The budget moves no money: `available` is exactly the pre-budget figure
    // minus the real 150 expense.
    assert_eq!(
        after.available,
        before.available - Money::from_minor_units(15_000, 2)
    );
    assert_eq!(after.reserved, Money::from_minor_units(25_000, 2)); // 400 - 150
    assert_eq!(after.free, after.available - after.reserved);

    let statuses = crate::commands::month_budgets_inner(&pool, account.id(), march)
        .await
        .unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].planned, Money::from_minor_units(40_000, 2));
    assert_eq!(statuses[0].actual, Money::from_minor_units(15_000, 2));
    assert_eq!(statuses[0].reserved, Money::from_minor_units(25_000, 2));
}

#[tokio::test]
async fn summary_account_concatenates_member_budget_statuses() {
    let (_dir, pool) = fixture().await;
    let a = insert_account(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let b = insert_account(&pool, &account_with("USD", "B"))
        .await
        .unwrap();
    let groceries = seed_category(&pool, "Groceries").await;

    // A: 400 budgeted, 100 spent -> reserves 300. B: 200 budgeted, 300 spent ->
    // overspent, reserves nothing. A merged-by-category row would claim 200.
    crate::commands::create_budget_inner(&pool, new_budget(a.id(), groceries.id(), 40_000))
        .await
        .unwrap();
    seed_categorized_expense(&pool, a.id(), groceries.id(), 10_000).await;
    crate::commands::create_budget_inner(&pool, new_budget(b.id(), groceries.id(), 20_000))
        .await
        .unwrap();
    seed_categorized_expense(&pool, b.id(), groceries.id(), 30_000).await;

    let summary = insert_account(&pool, &summary_over("USD", "All", vec![a.id(), b.id()]))
        .await
        .unwrap();
    let march = Month::new(2026, 3).unwrap();
    let statuses = crate::commands::month_budgets_inner(&pool, summary.id(), march)
        .await
        .unwrap();

    // One row per member, each tagged with its source account.
    assert_eq!(statuses.len(), 2);
    assert_eq!(statuses[0].account_id, a.id());
    assert_eq!(statuses[1].account_id, b.id());

    // The rows and the headline figure agree, because both are per-member sums.
    let total = statuses
        .iter()
        .fold(Money::zero(), |acc, s| acc + s.reserved);
    assert_eq!(total, Money::from_minor_units(30_000, 2));
    let combined = crate::commands::month_summary_inner(&pool, summary.id(), march)
        .await
        .unwrap();
    assert_eq!(combined.reserved, total);
    assert_eq!(combined.free, combined.available - combined.reserved);
}

#[tokio::test]
async fn backup_round_trip_preserves_budgets() {
    let (src_dir, src) = fixture().await;
    let account = seed_account(&src).await;
    let groceries = seed_category(&src, "Groceries").await;
    crate::commands::create_budget_inner(&src, new_budget(account.id(), groceries.id(), 40_000))
        .await
        .unwrap();
    let bytes = crate::backup::snapshot(&src, src_dir.path()).await.unwrap();

    let (dst_dir, dst) = fixture().await;
    crate::backup::restore(&dst, dst_dir.path(), &bytes)
        .await
        .unwrap();
    let restored = repo::budget::for_account(&dst, account.id()).await.unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(
        restored[0].base_amount(),
        Money::from_minor_units(40_000, 2)
    );
}

#[tokio::test]
async fn budget_before_the_account_anchor_is_inert() {
    let (_dir, pool) = fixture().await;
    // `seed_account` anchors at 2026-01, so 2025-12 predates the ledger.
    let account = seed_account(&pool).await;
    let groceries = seed_category(&pool, "Groceries").await;
    let mut draft = new_budget(account.id(), groceries.id(), 40_000);
    draft.start = Month::new(2025, 12).unwrap();
    crate::commands::create_budget_inner(&pool, draft)
        .await
        .unwrap();

    let before_anchor = Month::new(2025, 12).unwrap();
    let summary = crate::commands::month_summary_inner(&pool, account.id(), before_anchor)
        .await
        .unwrap();
    let statuses = crate::commands::month_budgets_inner(&pool, account.id(), before_anchor)
        .await
        .unwrap();

    // The rows and the headline must agree: a pre-anchor month reserves nothing.
    assert_eq!(summary.reserved, Money::zero());
    assert_eq!(summary.free, summary.available);
    assert_eq!(statuses, []);

    // From the anchor on, the same budget does apply.
    let anchored =
        crate::commands::month_budgets_inner(&pool, account.id(), Month::new(2026, 1).unwrap())
            .await
            .unwrap();
    assert_eq!(anchored.len(), 1);
}

#[tokio::test]
async fn budget_amount_history_round_trips_and_is_not_retroactive() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let groceries = seed_category(&pool, "Groceries").await;
    let budget = crate::commands::create_budget_inner(
        &pool,
        new_budget(account.id(), groceries.id(), 40_000),
    )
    .await
    .unwrap();
    assert_eq!(budget.amounts().len(), 1);

    // Raise to 450 from June, keeping the 400 base at the January start.
    let raised = talea_core::Budget::new_with_amounts(
        budget.id(),
        budget.account_id(),
        budget.category_id(),
        vec![
            talea_core::BudgetSegment::new(budget.start(), Money::from_minor_units(40_000, 2)),
            talea_core::BudgetSegment::new(
                Month::new(2026, 6).unwrap(),
                Money::from_minor_units(45_000, 2),
            ),
        ],
        budget.start(),
        None,
    )
    .unwrap();
    crate::commands::update_budget_inner(&pool, raised.clone())
        .await
        .unwrap();

    let loaded = repo::budget::for_account(&pool, account.id())
        .await
        .unwrap();
    assert_eq!(loaded, vec![raised]);

    // Each month reserves its own planned figure — the raise is not retroactive.
    let may_rows =
        crate::commands::month_budgets_inner(&pool, account.id(), Month::new(2026, 5).unwrap())
            .await
            .unwrap();
    let june_rows =
        crate::commands::month_budgets_inner(&pool, account.id(), Month::new(2026, 6).unwrap())
            .await
            .unwrap();
    assert_eq!(may_rows[0].planned, Money::from_minor_units(40_000, 2));
    assert_eq!(june_rows[0].planned, Money::from_minor_units(45_000, 2));

    // And the month summary's reservation follows the same figure.
    let may =
        crate::commands::month_summary_inner(&pool, account.id(), Month::new(2026, 5).unwrap())
            .await
            .unwrap();
    let june =
        crate::commands::month_summary_inner(&pool, account.id(), Month::new(2026, 6).unwrap())
            .await
            .unwrap();
    assert_eq!(may.reserved, Money::from_minor_units(40_000, 2));
    assert_eq!(june.reserved, Money::from_minor_units(45_000, 2));
}

#[tokio::test]
async fn updating_a_budget_replaces_its_breakpoints_wholesale() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let groceries = seed_category(&pool, "Groceries").await;
    let budget = crate::commands::create_budget_inner(
        &pool,
        new_budget(account.id(), groceries.id(), 40_000),
    )
    .await
    .unwrap();

    let with_breakpoint = talea_core::Budget::new_with_amounts(
        budget.id(),
        budget.account_id(),
        budget.category_id(),
        vec![
            talea_core::BudgetSegment::new(budget.start(), Money::from_minor_units(40_000, 2)),
            talea_core::BudgetSegment::new(
                Month::new(2026, 6).unwrap(),
                Money::from_minor_units(45_000, 2),
            ),
        ],
        budget.start(),
        None,
    )
    .unwrap();
    crate::commands::update_budget_inner(&pool, with_breakpoint)
        .await
        .unwrap();

    // Saving a single-segment history again must clear the stored breakpoint,
    // not leave it behind (the "all months" edit scope relies on this).
    crate::commands::update_budget_inner(&pool, budget.clone())
        .await
        .unwrap();
    let loaded = repo::budget::for_account(&pool, account.id())
        .await
        .unwrap();
    assert_eq!(loaded, vec![budget]);
}

#[tokio::test]
async fn deleting_a_budget_cascades_its_amount_history() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let groceries = seed_category(&pool, "Groceries").await;
    let budget = crate::commands::create_budget_inner(
        &pool,
        new_budget(account.id(), groceries.id(), 40_000),
    )
    .await
    .unwrap();
    let raised = talea_core::Budget::new_with_amounts(
        budget.id(),
        budget.account_id(),
        budget.category_id(),
        vec![
            talea_core::BudgetSegment::new(budget.start(), Money::from_minor_units(40_000, 2)),
            talea_core::BudgetSegment::new(
                Month::new(2026, 6).unwrap(),
                Money::from_minor_units(45_000, 2),
            ),
        ],
        budget.start(),
        None,
    )
    .unwrap();
    crate::commands::update_budget_inner(&pool, raised)
        .await
        .unwrap();

    let budget_key = i64::try_from(budget.id().get()).unwrap();
    assert!(repo::budget::delete(&pool, budget.id()).await.unwrap());
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM budget_amount WHERE budget_id = ?")
        .bind(budget_key)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

/// Inserts a `minor` entry of `kind` dated `on`.
async fn seed_entry_on(
    pool: &SqlitePool,
    account_id: AccountId,
    minor: i64,
    kind: EntryKind,
    on: Date,
) {
    let draft = NewEntry {
        date: on,
        ..new_entry(account_id, minor, kind)
    }
    .build()
    .unwrap();
    repo::entry::insert(pool, &draft).await.unwrap();
}

#[tokio::test]
async fn balance_as_of_counts_entries_up_to_the_date() {
    let (_dir, pool) = fixture().await;
    let id = seed_account(&pool).await.id();
    let march = |d| date(2026, TMonth::March, d);
    seed_entry_on(&pool, id, 100_000, EntryKind::Income, march(1)).await; // +1000
    seed_entry_on(&pool, id, 20_000, EntryKind::Expense, march(15)).await; // -200
    seed_entry_on(&pool, id, 30_000, EntryKind::Expense, march(25)).await; // -300

    let as_of = |d| crate::commands::balance_as_of_inner(&pool, id, march(d));
    assert_eq!(
        as_of(14).await.unwrap(),
        Money::from_minor_units(100_000, 2)
    );
    // The date itself counts; the 25th's expense is still ahead.
    assert_eq!(as_of(15).await.unwrap(), Money::from_minor_units(80_000, 2));
    // On the month's last day it is exactly that month's `available`.
    let summary = crate::commands::month_summary_inner(&pool, id, Month::new(2026, 3).unwrap())
        .await
        .unwrap();
    assert_eq!(as_of(31).await.unwrap(), summary.available);
}

#[tokio::test]
async fn summary_account_balance_as_of_sums_its_members() {
    let (_dir, pool) = fixture().await;
    let a = insert_account(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let b = insert_account(&pool, &account_with("USD", "B"))
        .await
        .unwrap();
    let march = |d| date(2026, TMonth::March, d);
    seed_entry_on(&pool, a.id(), 50_000, EntryKind::Income, march(1)).await; // +500 on A
    seed_entry_on(&pool, b.id(), 10_000, EntryKind::Expense, march(2)).await; // -100 on B

    // After the cut-off, so neither member counts it yet.
    seed_entry_on(&pool, b.id(), 99_900, EntryKind::Expense, march(20)).await;
    let summary = insert_account(&pool, &summary_over("USD", "All", vec![a.id(), b.id()]))
        .await
        .unwrap();

    let as_of = |id| crate::commands::balance_as_of_inner(&pool, id, march(10));
    let total = as_of(summary.id()).await.unwrap();
    assert_eq!(total, Money::from_minor_units(40_000, 2)); // 500 - 100
    assert_eq!(
        total,
        as_of(a.id()).await.unwrap() + as_of(b.id()).await.unwrap()
    );
}

#[tokio::test]
async fn balance_as_of_a_missing_account_is_not_found() {
    let (_dir, pool) = fixture().await;
    let result = crate::commands::balance_as_of_inner(
        &pool,
        AccountId::new(999),
        date(2026, TMonth::March, 10),
    )
    .await;
    assert!(matches!(result, Err(CommandError::NotFound)));
}

#[test]
fn iso_date_argument_is_a_bare_iso_string() {
    // Exactly what the frontend sends for `balance_as_of`'s `date` argument.
    let parsed: IsoDate = serde_json::from_str(r#""2026-03-15""#).expect("ISO date parses");
    assert_eq!(parsed.get(), date(2026, TMonth::March, 15));
    // Tauri hands each command argument over as a `serde_json::Value`.
    let from_value: IsoDate = serde_json::from_value(serde_json::json!("2026-03-15")).unwrap();
    assert_eq!(from_value.get(), parsed.get());
    // The domain's year range is the boundary's too: `time` alone admits ±9999.
    for edge in [r#""0001-01-01""#, r#""9999-12-31""#] {
        assert!(
            serde_json::from_str::<IsoDate>(edge).is_ok(),
            "{edge} is in range"
        );
    }
    for bad in [
        r#""2026/03/15""#,
        r#""2026-02-30""#,
        "20260315",
        r#""0000-12-31""#,
        r#""-0001-06-15""#,
    ] {
        assert!(
            serde_json::from_str::<IsoDate>(bad).is_err(),
            "{bad} must be rejected"
        );
    }
}

#[tokio::test]
async fn an_out_of_range_opening_balance_reads_as_corrupt_not_an_abort() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    seed_entry(&pool, account.id(), 100, EntryKind::Income).await;
    // What a tampered backup could hold: restore copies rows verbatim, past every
    // constructor. Summed with that entry, `Decimal::MAX` overflows the ledger's
    // panic-on-overflow arithmetic, and release builds abort on panic.
    sqlx::query("UPDATE account SET opening_balance = ? WHERE id = ?")
        .bind("79228162514264337593543950335")
        .bind(i64::try_from(account.id().get()).unwrap())
        .execute(&pool)
        .await
        .unwrap();

    let today = date(2026, TMonth::March, 10);
    let balance = crate::commands::balance_as_of_inner(&pool, account.id(), today).await;
    assert!(matches!(balance, Err(CommandError::Corrupt)));
    let march = Month::new(2026, 3).unwrap();
    let summary = crate::commands::month_summary_inner(&pool, account.id(), march).await;
    assert!(matches!(summary, Err(CommandError::Corrupt)));
}

#[tokio::test]
async fn a_full_summary_at_the_opening_balance_cap_sums_without_overflow() {
    let (_dir, pool) = fixture().await;
    let cap = Money::from_minor_units(1_000_000_000_000_000, 0);
    let mut members = Vec::new();
    for n in 0..talea_core::domain::error::MAX_SUMMARY_MEMBERS {
        let account = Account::new(
            AccountId::new(0),
            format!("Member {n}"),
            "💰".to_owned(),
            Currency::new("USD").unwrap(),
            cap,
            Month::new(2026, 1).unwrap(),
        )
        .unwrap();
        members.push(insert_account(&pool, &account).await.unwrap().id());
    }
    let count = i64::try_from(members.len()).unwrap();
    let summary = insert_account(&pool, &summary_over("USD", "All", members))
        .await
        .unwrap();

    let total =
        crate::commands::balance_as_of_inner(&pool, summary.id(), date(2026, TMonth::March, 10))
            .await
            .unwrap();
    assert_eq!(
        total,
        Money::from_minor_units(1_000_000_000_000_000 * count, 0)
    );
}

#[test]
fn new_account_payload_caps_the_opening_balance() {
    // `NewAccount::build` inherits the cap from `Account::build`; pin it at the
    // boundary the frontend actually calls, in both directions.
    for opening in ["1000000000000000.01", "-1000000000000000.01"] {
        let json = format!(
            r#"{{"name":"Personal","icon":"💰","currency":"USD",
            "opening_balance":"{opening}","anchor":{{"year":2026,"month":5}}}}"#
        );
        let draft = serde_json::from_str::<crate::dto::NewAccount>(&json).expect("parses");
        assert!(matches!(
            draft.build(),
            Err(talea_core::DomainError::OpeningBalanceTooLarge(_))
        ));
    }
}

/// Saves the Nextcloud settings as `save_settings` does when the user confirms
/// any new server. It runs on a thread of its own, with its own runtime, so a
/// test can call it from inside an async test, or from a confirmation or an
/// upload that test injects.
fn save(dir: &Path, base_url: &str, username: &str, password: &str) -> Result<(), CommandError> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(crate::backup::save_settings(
                        dir,
                        base_url,
                        username,
                        password,
                        |_server| async { Ok(true) },
                    ))
            })
            .join()
            .unwrap()
    })
    .map(|saved| assert!(saved, "a confirmed save saves"))
}

/// The new server a save of `base_url` and `username` would ask the user to
/// confirm, if any: `None` keeps the stored one, or disconnects.
fn new_server(
    stored: &crate::backup::Destination,
    base_url: &str,
    username: &str,
) -> Result<Option<crate::backup::NewServer>, CommandError> {
    Ok(match crate::backup::change(stored, base_url, username)? {
        crate::backup::Change::Move(server) => Some(server),
        crate::backup::Change::Keep | crate::backup::Change::Disconnect => None,
    })
}

/// A fresh app-data dir holding saved Nextcloud credentials and a past backup.
fn nextcloud_dir() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    save(dir.path(), "https://cloud.example.com", "max", "s3cret").unwrap();
    let destination = crate::backup::load_config(dir.path()).destination();
    crate::backup::mark_backed_up(dir.path(), &destination).unwrap();
    dir
}

#[test]
fn a_blank_password_keeps_the_stored_one_for_the_same_server_and_user() {
    let dir = nextcloud_dir();
    // Whitespace around the address is trimmed, as on every save.
    save(dir.path(), " https://cloud.example.com ", "max", "").unwrap();
    let config = crate::backup::load_config(dir.path());
    assert_eq!(config.base_url, "https://cloud.example.com");
    assert_eq!(config.password, "s3cret");
    // Same server, so the backup it holds is still the last one.
    assert!(config.last_backup.is_some());
}

#[test]
fn a_blank_password_is_dropped_once_the_server_or_user_changes() {
    for (base_url, username) in [
        ("https://attacker.example", "max"),
        ("https://cloud.example.com", "someone-else"),
        // Written differently counts as different: re-entering is the cheap side.
        ("https://cloud.example.com/", "max"),
        ("https://Cloud.example.com", "max"),
        ("https://cloud.example.co", "max"),
    ] {
        let dir = nextcloud_dir();
        save(dir.path(), base_url, username, "").unwrap();
        // The new address is saved, but the old server's password didn't follow
        // it there, so nothing can be sent until the new one is entered.
        let config = crate::backup::load_config(dir.path());
        assert_eq!(config.base_url, base_url, "{base_url} / {username}");
        assert_eq!(config.username, username);
        assert_eq!(config.password, "");
        assert!(!config.is_configured());
        // The last backup lives on the old server, not this one.
        assert_eq!(config.last_backup, None);
    }
}

#[test]
fn clearing_the_address_and_user_disconnects() {
    let dir = nextcloud_dir();
    save(dir.path(), "", "", "").unwrap();
    let config = crate::backup::load_config(dir.path());
    assert_eq!(config.password, "");
    assert_eq!(config.last_backup, None);
    assert!(!config.is_configured());
}

#[test]
fn a_password_typed_while_disconnecting_is_not_kept() {
    // With no address there is no one to give it to.
    let dir = tempfile::tempdir().unwrap();
    save(dir.path(), "", "", "s3cret").unwrap();
    assert_eq!(crate::backup::load_config(dir.path()).password, "");
}

#[test]
fn a_legacy_password_beside_no_address_follows_nowhere() {
    // The old rule let clearing both fields keep the password, leaving configs
    // like this behind. Nothing can produce one now, so it is written directly.
    let legacy = || {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("nextcloud.json"),
            r#"{"baseUrl":"","username":"","password":"s3cret"}"#,
        )
        .unwrap();
        assert_eq!(crate::backup::load_config(dir.path()).password, "s3cret");
        dir
    };
    // Neither the next address typed in...
    let dir = legacy();
    save(dir.path(), "https://attacker.example", "max", "").unwrap();
    assert_eq!(crate::backup::load_config(dir.path()).password, "");
    // ...nor "the same" empty pair saved again may keep it.
    let dir = legacy();
    save(dir.path(), "", "", "").unwrap();
    assert_eq!(crate::backup::load_config(dir.path()).password, "");
}

#[test]
fn the_password_never_shows_in_debug_output() {
    let config = crate::backup::load_config(nextcloud_dir().path());
    let printed = format!("{config:?}");
    assert!(!printed.contains("s3cret"), "{printed}");
    assert!(printed.contains("[redacted]"));
}

#[test]
fn a_new_password_moves_the_credentials_to_a_new_server() {
    let dir = nextcloud_dir();
    save(dir.path(), "https://new.example.com", "max", "fresh").unwrap();
    let config = crate::backup::load_config(dir.path());
    assert_eq!(config.base_url, "https://new.example.com");
    assert_eq!(config.password, "fresh");
}

#[test]
fn a_first_save_without_a_password_is_allowed() {
    // Nothing stored yet, so nothing can follow the address anywhere.
    let dir = tempfile::tempdir().unwrap();
    save(dir.path(), "https://cloud.example.com", "max", "").unwrap();
    let config = crate::backup::load_config(dir.path());
    assert_eq!(config.base_url, "https://cloud.example.com");
    assert_eq!(config.username, "max");
    assert!(!config.is_configured());
}

#[test]
fn a_password_stored_beside_an_empty_field_still_stays_put() {
    // A blank field is no "first save" once a password is stored next to it:
    // before this rule, clearing just the address kept the password, so such
    // configs exist, and the password must not follow whatever is typed next.
    // No save can make one now, so each is written directly.
    for (first, second) in [
        (("", "max"), ("https://attacker.example", "max")),
        (
            ("https://cloud.example.com", ""),
            ("https://cloud.example.com", "someone-else"),
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let legacy = serde_json::json!({
            "baseUrl": first.0,
            "username": first.1,
            "password": "s3cret",
        });
        std::fs::write(dir.path().join("nextcloud.json"), legacy.to_string()).unwrap();
        // The legacy config loads as written, password and all.
        assert_eq!(crate::backup::load_config(dir.path()).password, "s3cret");
        save(dir.path(), second.0, second.1, "").unwrap();
        assert_eq!(
            crate::backup::load_config(dir.path()).password,
            "",
            "{first:?}"
        );
    }
}

/// The stored `nextcloud.json`, byte for byte.
fn stored_settings(dir: &Path) -> Vec<u8> {
    std::fs::read(dir.join("nextcloud.json")).unwrap()
}

#[test]
fn a_backup_is_recorded_only_against_the_server_it_went_to() {
    // The settings move to another server, another user, or nowhere while
    // the upload runs.
    for (base_url, username) in [
        ("https://other.example.com", "max"),
        ("https://cloud.example.com", "someone-else"),
        ("", ""),
    ] {
        let dir = nextcloud_dir();
        let uploaded_to = crate::backup::load_config(dir.path()).destination();
        save(dir.path(), base_url, username, "other").unwrap();
        let before = stored_settings(dir.path());
        let stamp = crate::backup::mark_backed_up(dir.path(), &uploaded_to).unwrap();
        assert_eq!(stamp, None, "{base_url} / {username}");
        // Nor does recording it bring the old server back.
        assert_eq!(stored_settings(dir.path()), before);
    }
    // A new password for the same server is still the same destination.
    let dir = nextcloud_dir();
    let uploaded_to = crate::backup::load_config(dir.path()).destination();
    save(dir.path(), "https://cloud.example.com", "max", "newer").unwrap();
    let stamp = crate::backup::mark_backed_up(dir.path(), &uploaded_to).unwrap();
    let config = crate::backup::load_config(dir.path());
    assert!(stamp.is_some());
    assert_eq!(config.last_backup, stamp);
    assert_eq!(config.password, "newer");
}

#[test]
fn only_a_new_server_needs_confirming() {
    let stored = crate::backup::load_config(nextcloud_dir().path()).destination();
    let new_server = |base_url: &str, username: &str| new_server(&stored, base_url, username);
    // The stored server, however it is padded, and a disconnect send nothing
    // anywhere new.
    for (base_url, username) in [
        ("https://cloud.example.com", "max"),
        (" https://cloud.example.com ", " max "),
        ("", ""),
        ("  ", " "),
    ] {
        assert!(
            new_server(base_url, username).unwrap().is_none(),
            "{base_url:?} / {username:?}"
        );
    }
    // Any other server or user does, written differently or not, and shown
    // as requests would reach it.
    for (base_url, username, address, shown_user) in [
        (
            "https://cloud.example.com",
            "someone-else",
            "https://cloud.example.com",
            "someone-else",
        ),
        (
            "https://cloud.example.com/",
            "max",
            "https://cloud.example.com",
            "max",
        ),
        (
            "https://Cloud.example.com",
            "max",
            "https://cloud.example.com",
            "max",
        ),
        (
            "https://cloud.example.com",
            "Max",
            "https://cloud.example.com",
            "Max",
        ),
        (
            "https://cloud.example.com/nc",
            "max",
            "https://cloud.example.com/nc",
            "max",
        ),
        (
            "  https://other.example.com:8443  ",
            "  max  ",
            "https://other.example.com:8443",
            "max",
        ),
    ] {
        let server = new_server(base_url, username).unwrap().unwrap();
        assert_eq!(server.address(), address, "{base_url:?} / {username:?}");
        assert_eq!(server.username(), shown_user);
    }
    // A server to confirm must be named outright, address and user both.
    for (base_url, username) in [
        ("http://cloud.example.com", "max"),
        ("cloud.example.com", "max"),
        ("", "max"),
        ("https://cloud.example.com", ""),
        ("https://new.example.com", "  "),
        ("https://cloud.example.com@evil.example", "max"),
    ] {
        assert!(
            matches!(new_server(base_url, username), Err(CommandError::Backup(_))),
            "{base_url:?} / {username:?}"
        );
    }
}

#[test]
fn a_new_username_must_be_a_nextcloud_user_id() {
    let stored = crate::backup::Destination::default();
    let new_server = |username: &str| new_server(&stored, "https://cloud.example.com", username);
    let longest = "m".repeat(64);
    for username in [
        "max",
        "o'brien",
        "first.last",
        "a-b_c",
        "two words",
        "max@example.com",
        &longest,
    ] {
        assert!(new_server(username).unwrap().is_some(), "{username:?}");
    }
    for username in [
        // A line break could fake a line of the dialog's own...
        "max\nOnly continue",
        "max\u{2028}x",
        // ...a direction mark or an invisible character disguise the name...
        "x\u{202e}moc",
        "x\u{2066}y",
        "x\u{200e}y",
        "ma\u{200b}x",
        "max\u{0}",
        "max\u{7f}",
        "max\u{a0}x",
        // ...a letter from another script pass for a Latin one...
        "m\u{430}x",
        "Jose\u{301}",
        "José",
        // ...a `%` read as formatting by the Linux dialog...
        "%s%n",
        // ...or anything else Nextcloud doesn't put in a user id.
        ".",
        "..",
        r"dom\max",
        "\"max\"",
        "max+nc@example.com",
        // A long one could hide its end.
        &format!("{longest}m"),
    ] {
        assert!(
            matches!(new_server(username), Err(CommandError::Backup(_))),
            "{username:?}"
        );
    }
}

#[test]
fn the_confirmation_and_the_password_rule_agree_on_what_is_the_same_server() {
    // A save that asks nothing must be one that keeps the stored password, and
    // one that asks must drop it: the two rules mustn't drift apart.
    for (base_url, username) in [
        ("https://cloud.example.com", "max"),
        (" https://cloud.example.com", "max "),
        ("https://cloud.example.com/", "max"),
        ("https://Cloud.example.com", "max"),
        ("https://cloud.example.com", "Max"),
        ("https://cloud.example.com", "someone-else"),
        ("https://other.example.com", "max"),
    ] {
        let dir = nextcloud_dir();
        let stored = crate::backup::load_config(dir.path()).destination();
        let asks = new_server(&stored, base_url, username).unwrap().is_some();
        save(dir.path(), base_url, username, "").unwrap();
        let kept = crate::backup::load_config(dir.path()).password == "s3cret";
        assert_ne!(asks, kept, "{base_url:?} / {username:?}");
    }
}

#[test]
fn a_server_that_cant_be_shown_is_never_stored() {
    // Even confirmed: the dialog couldn't have shown it as it is.
    let dir = nextcloud_dir();
    let before = stored_settings(dir.path());
    for (base_url, username) in [
        ("https://cloud.example.com@evil.example", "max"),
        ("https://other.example.com", "ma\nx"),
        ("https://other.example.com", ""),
        ("", "max"),
    ] {
        assert!(
            matches!(
                save(dir.path(), base_url, username, "x"),
                Err(CommandError::Backup(_))
            ),
            "{base_url:?} / {username:?}"
        );
        assert_eq!(stored_settings(dir.path()), before);
    }
}

#[test]
fn a_first_server_needs_confirming_too() {
    let stored = crate::backup::load_config(tempfile::tempdir().unwrap().path()).destination();
    let server = new_server(&stored, "https://cloud.example.com", "max")
        .unwrap()
        .unwrap();
    assert_eq!(server.address(), "https://cloud.example.com");
}

#[test]
fn an_overlong_password_is_not_stored() {
    let dir = nextcloud_dir();
    let before = stored_settings(dir.path());
    // The cap counts bytes, so 513 two-byte characters are already too many.
    for password in ["p".repeat(1_025), "é".repeat(513)] {
        let refused = save(dir.path(), "https://cloud.example.com", "max", &password);
        assert!(matches!(refused, Err(CommandError::Backup(_))));
        assert_eq!(stored_settings(dir.path()), before);
    }
    save(
        dir.path(),
        "https://cloud.example.com",
        "max",
        &"p".repeat(1_024),
    )
    .unwrap();
}

#[tokio::test]
async fn an_overlong_password_is_refused_before_anything_is_asked() {
    let dir = tempfile::tempdir().unwrap();
    let mut asked = false;
    let refused = crate::backup::save_settings(
        dir.path(),
        "https://cloud.example.com",
        "max",
        &"p".repeat(1_025),
        |_server| {
            asked = true;
            async { Ok(true) }
        },
    )
    .await;
    assert!(refused.is_err());
    assert!(!asked);
}

/// Saves through `save_settings`, answering a confirmation with `answer`;
/// returns the save's result and the servers it asked about.
async fn save_answering(
    dir: &Path,
    base_url: &str,
    username: &str,
    answer: Result<bool, CommandError>,
) -> (Result<bool, CommandError>, Vec<String>) {
    let mut asked = Vec::new();
    let saved = crate::backup::save_settings(dir, base_url, username, "fresh", |server| {
        asked.push(format!("{} / {}", server.address(), server.username()));
        async { answer }
    })
    .await;
    (saved, asked)
}

#[tokio::test]
async fn a_new_server_is_saved_once_confirmed() {
    let dir = tempfile::tempdir().unwrap();
    let (saved, asked) =
        save_answering(dir.path(), "https://cloud.example.com", "max", Ok(true)).await;
    assert!(saved.unwrap());
    assert_eq!(asked, ["https://cloud.example.com / max"]);
    let config = crate::backup::load_config(dir.path());
    assert_eq!(config.base_url, "https://cloud.example.com");
    assert_eq!(config.password, "fresh");
}

#[tokio::test]
async fn a_declined_or_failed_confirmation_saves_nothing() {
    for answer in [Ok(false), Err(CommandError::Backup("no dialog".into()))] {
        let declined = matches!(answer, Ok(false));
        let dir = nextcloud_dir();
        let before = stored_settings(dir.path());
        let (saved, asked) =
            save_answering(dir.path(), "https://other.example.com", "max", answer).await;
        // A decline is an answer, which the screen reports as not saved; a
        // failed confirmation is an error.
        if declined {
            assert!(matches!(saved, Ok(false)), "{saved:?}");
        } else {
            assert!(matches!(saved, Err(CommandError::Backup(_))), "{saved:?}");
        }
        assert_eq!(asked.len(), 1);
        assert_eq!(stored_settings(dir.path()), before);
    }
}

#[tokio::test]
async fn the_stored_server_saves_without_asking() {
    // A new password for the stored server is saved...
    let dir = nextcloud_dir();
    let (saved, asked) =
        save_answering(dir.path(), "https://cloud.example.com", "max", Ok(false)).await;
    assert!(saved.unwrap());
    assert_eq!(asked, Vec::<String>::new());
    let config = crate::backup::load_config(dir.path());
    assert_eq!(config.password, "fresh");
    assert!(config.last_backup.is_some());
    // ...and so is a disconnect, which drops the password with the server.
    let dir = nextcloud_dir();
    let (saved, asked) = save_answering(dir.path(), "", "", Ok(false)).await;
    assert!(saved.unwrap());
    assert_eq!(asked, Vec::<String>::new());
    let config = crate::backup::load_config(dir.path());
    assert_eq!(config.base_url, "");
    assert_eq!(config.password, "");
    assert_eq!(config.last_backup, None);
}

#[tokio::test]
async fn a_server_that_cant_be_confirmed_is_not_asked_about() {
    let dir = nextcloud_dir();
    let before = stored_settings(dir.path());
    let (saved, asked) = save_answering(
        dir.path(),
        "https://cloud.example.com@evil.example",
        "max",
        Ok(true),
    )
    .await;
    assert!(saved.is_err());
    assert_eq!(asked, Vec::<String>::new());
    assert_eq!(stored_settings(dir.path()), before);
}

#[tokio::test]
async fn a_confirmation_overtaken_by_another_save_saves_nothing() {
    // Whether the save needed confirming was decided against the stored
    // server; once another save has moved it, whether the address moved or
    // only the user, that no longer holds. A first save is no exception.
    for (start, base_url, username) in [
        (nextcloud_dir(), "https://other.example.com", "max"),
        (nextcloud_dir(), "https://cloud.example.com", "someone-else"),
        (
            tempfile::tempdir().unwrap(),
            "https://other.example.com",
            "max",
        ),
    ] {
        let dir = start.path();
        let mut overtaken = None;
        let saved = crate::backup::save_settings(
            dir,
            "https://new.example.com",
            "max",
            "fresh",
            |_server| {
                // While the dialog is open, another save moves the settings.
                save(dir, base_url, username, "other").unwrap();
                overtaken = Some(stored_settings(dir));
                async { Ok(true) }
            },
        )
        .await;
        assert!(
            matches!(&saved, Err(CommandError::Backup(message)) if message.contains("changed meanwhile")),
            "{saved:?}"
        );
        assert_eq!(
            Some(stored_settings(dir)),
            overtaken,
            "{base_url} / {username}"
        );
    }
}

#[tokio::test]
async fn a_backup_is_credited_to_the_server_it_was_uploaded_to() {
    let dir = nextcloud_dir();
    let stamp = crate::backup::back_up(dir.path(), |config| {
        assert_eq!(config.base_url, "https://cloud.example.com");
        assert_eq!(config.username, "max");
        assert_eq!(config.password, "s3cret");
        async { Ok(()) }
    })
    .await
    .unwrap();
    assert!(stamp.is_some());
    assert_eq!(crate::backup::load_config(dir.path()).last_backup, stamp);
}

#[tokio::test]
async fn a_backup_overtaken_by_a_new_server_is_not_credited_to_it() {
    let dir = nextcloud_dir();
    // The settings move to another server while the upload runs.
    let stamp = crate::backup::back_up(dir.path(), |_config| {
        save(dir.path(), "https://other.example.com", "max", "other").unwrap();
        async { Ok(()) }
    })
    .await
    .unwrap();
    assert_eq!(stamp, None);
    let config = crate::backup::load_config(dir.path());
    assert_eq!(config.base_url, "https://other.example.com");
    assert_eq!(config.last_backup, None);
}

#[test]
fn a_backup_whose_settings_cant_be_read_isnt_put_down_to_a_move() {
    // Unreadable settings haven't moved to another server: recording the
    // time failed, and the screen must say that rather than "elsewhere".
    let dir = nextcloud_dir();
    let uploaded_to = crate::backup::load_config(dir.path()).destination();
    std::fs::write(dir.path().join("nextcloud.json"), b"{ not json").unwrap();
    let recorded = crate::backup::mark_backed_up(dir.path(), &uploaded_to);
    assert!(
        matches!(&recorded, Err(CommandError::Backup(message)) if message.contains("couldn't record")),
        "{recorded:?}"
    );
}

#[tokio::test]
async fn a_failed_upload_is_not_recorded() {
    let dir = tempfile::tempdir().unwrap();
    save(dir.path(), "https://cloud.example.com", "max", "s3cret").unwrap();
    let failed = crate::backup::back_up(dir.path(), |_config| async {
        Err(CommandError::Backup("offline".into()))
    })
    .await;
    assert!(failed.is_err());
    assert_eq!(crate::backup::load_config(dir.path()).last_backup, None);
}

#[cfg(unix)]
#[test]
fn saving_the_settings_replaces_the_file_whole() {
    // Written beside the config and renamed over it, so a crash mid-write
    // can't truncate it; a temporary file a crash left behind is overwritten.
    use std::os::unix::fs::MetadataExt;
    let dir = nextcloud_dir();
    let config = dir.path().join("nextcloud.json");
    let before = std::fs::metadata(&config).unwrap().ino();
    let temp = dir.path().join("nextcloud.json.tmp");
    std::fs::write(&temp, b"half a write").unwrap();
    save(dir.path(), "https://cloud.example.com", "max", "n3w").unwrap();
    // A new file took the old one's place; an in-place write keeps the inode.
    assert_ne!(std::fs::metadata(&config).unwrap().ino(), before);
    assert!(!temp.exists());
    assert_eq!(crate::backup::load_config(dir.path()).password, "n3w");
}

#[cfg(unix)]
#[test]
fn the_credentials_file_is_readable_by_its_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = nextcloud_dir();
    let mode = std::fs::metadata(dir.path().join("nextcloud.json"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

// ---- Restore validation -----------------------------------------------------

/// How each kind of refusal reads to the user.
const NOT_A_BACKUP: &str = "isn't a Talea backup";
const UNREADABLE: &str = "can't read";
const OTHER_VERSION: &str = "different app version";
const OVERSIZED: &str = "more data than Talea can handle";

/// A database with a row in every table restore copies: the baseline each
/// forged backup below breaks in exactly one place.
async fn populated_source() -> (TempDir, SqlitePool) {
    let (dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let groceries = seed_category(&pool, "Groceries").await;
    seed_categorized_expense(&pool, account.id(), groceries.id(), 2_500).await;
    let start = date(2026, TMonth::January, 5);
    let rule = RecurringRule::new_with_amounts(
        RecurringRuleId::new(0),
        account.id(),
        vec![
            AmountSegment::new(start, Money::from_minor_units(5_000, 2)),
            AmountSegment::new(
                date(2026, TMonth::June, 1),
                Money::from_minor_units(6_000, 2),
            ),
        ],
        EntryKind::Expense,
        None,
        None,
        start,
        RuleEnd::Never,
        Frequency::new(FreqUnit::Monthly, 1).unwrap(),
    )
    .unwrap();
    let rule = repo::rule::insert(&pool, &rule).await.unwrap();
    repo::skip::add(&pool, rule.id(), date(2026, TMonth::February, 5))
        .await
        .unwrap();
    let budget = crate::commands::create_budget_inner(
        &pool,
        new_budget(account.id(), groceries.id(), 40_000),
    )
    .await
    .unwrap();
    let raised = talea_core::Budget::new_with_amounts(
        budget.id(),
        budget.account_id(),
        budget.category_id(),
        vec![
            talea_core::BudgetSegment::new(budget.start(), Money::from_minor_units(40_000, 2)),
            talea_core::BudgetSegment::new(
                Month::new(2026, 6).unwrap(),
                Money::from_minor_units(45_000, 2),
            ),
        ],
        budget.start(),
        None,
    )
    .unwrap();
    crate::commands::update_budget_inner(&pool, raised)
        .await
        .unwrap();
    insert_account(&pool, &summary_over("USD", "All", vec![account.id()]))
        .await
        .unwrap();
    (dir, pool)
}

/// Each table restore copies, row by row, then the AUTOINCREMENT counters:
/// enough to tell whether a restore replaced anything, and whether what it
/// restored is what was backed up.
async fn table_state(pool: &SqlitePool) -> Vec<(String, Vec<String>)> {
    let mut state = Vec::new();
    for table in crate::backup::TABLES {
        let name = table.name;
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
                .bind(name)
                .fetch_all(pool)
                .await
                .unwrap();
        // Safe: the table and its columns come from this app's own schema.
        let rows: Vec<String> = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT json_array({}) FROM {name} ORDER BY rowid",
            columns.join(", ")
        )))
        .fetch_all(pool)
        .await
        .unwrap();
        state.push((name.to_owned(), rows));
    }
    let counters: Vec<String> =
        sqlx::query_scalar("SELECT json_array(name, seq) FROM sqlite_sequence ORDER BY name")
            .fetch_all(pool)
            .await
            .unwrap();
    state.push(("sqlite_sequence".to_owned(), counters));
    state
}

/// A real snapshot of `pool`, edited with `sql` the way a hand-edited or
/// tampered backup would be. Foreign keys are off while editing, so a test can
/// plant rows that break them.
async fn forged_backup(pool: &SqlitePool, dir: &Path, sql: &str) -> Vec<u8> {
    let bytes = crate::backup::snapshot(pool, dir).await.unwrap();
    let path = dir.join("forged.sqlite3");
    std::fs::write(&path, &bytes).unwrap();
    // A rollback journal, as a real backup has, so the edit lands in the file.
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .journal_mode(SqliteJournalMode::Delete)
        .foreign_keys(false);
    let forged = SqlitePool::connect_with(options).await.unwrap();
    // Safe: a test's own forgery, built from literals and `TABLES` names.
    sqlx::raw_sql(AssertSqlSafe(sql.to_owned()))
        .execute(&forged)
        .await
        .unwrap();
    forged.close().await;
    std::fs::read(&path).unwrap()
}

/// Restores `bytes` over a database holding one account unlike any a forged
/// backup holds, expects a refusal saying `expected`, and checks nothing local
/// changed: the account, and every table's row count and counter. `case` names
/// the forgery in a failure.
async fn assert_restore_refused(case: &str, bytes: &[u8], expected: &str) {
    let (dir, pool) = fixture().await;
    let local = insert_account(&pool, &account_with("EUR", "Local"))
        .await
        .unwrap();
    let before = table_state(&pool).await;
    let result = crate::backup::restore(&pool, dir.path(), bytes).await;
    assert!(
        matches!(&result, Err(CommandError::Backup(message)) if message.contains(expected)),
        "{case}: expected a refusal saying {expected:?}, got {result:?}"
    );
    assert_eq!(
        repo::account::list(&pool).await.unwrap(),
        vec![local],
        "{case}"
    );
    assert_eq!(table_state(&pool).await, before, "{case}");
}

/// Restores `bytes` into a fresh database, which must accept it.
async fn restored(bytes: &[u8]) -> (TempDir, SqlitePool) {
    let (dir, pool) = fixture().await;
    crate::backup::restore(&pool, dir.path(), bytes)
        .await
        .unwrap();
    (dir, pool)
}

#[tokio::test]
async fn a_backup_with_a_row_in_every_table_restores_whole() {
    // The baseline itself must pass, or every refusal below would prove nothing.
    let (dir, src) = populated_source().await;
    let bytes = crate::backup::snapshot(&src, dir.path()).await.unwrap();
    let (dst_dir, dst) = restored(&bytes).await;
    let state = table_state(&dst).await;
    assert_eq!(state, table_state(&src).await);
    assert!(state.iter().all(|(_, rows)| !rows.is_empty()), "{state:?}");
    // No scratch copy, nor any sidecar of one, is left behind.
    let left: Vec<String> = std::fs::read_dir(dst_dir.path())
        .unwrap()
        .map(|file| file.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("talea-backup-"))
        .collect();
    assert_eq!(left, Vec::<String>::new());
}

#[tokio::test]
async fn restore_refuses_a_schema_version_that_isnt_a_number() {
    // Decoding it used to panic, and release builds abort on panic.
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    let bytes = forged_backup(
        &src,
        dir.path(),
        "UPDATE _sqlx_migrations SET version = 'abc' \
         WHERE version = (SELECT MAX(version) FROM _sqlx_migrations)",
    )
    .await;
    assert_restore_refused("text version", &bytes, NOT_A_BACKUP).await;
}

#[tokio::test]
async fn restore_refuses_a_file_that_isnt_sqlite() {
    assert_restore_refused("not SQLite", b"not a database", NOT_A_BACKUP).await;
    // Nor one that only starts like a database.
    let mut bytes = b"SQLite format 3\0".to_vec();
    bytes.resize(8192, 0);
    assert_restore_refused("a header, then zeros", &bytes, NOT_A_BACKUP).await;
}

#[tokio::test]
async fn restore_names_a_newer_backup_as_another_version() {
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    // The other columns are copied from a real migration row, so the table
    // stays well-formed.
    let bytes = forged_backup(
        &src,
        dir.path(),
        "INSERT INTO _sqlx_migrations \
         (version, description, installed_on, success, checksum, execution_time) \
         SELECT 9999, description, installed_on, success, checksum, execution_time \
         FROM _sqlx_migrations LIMIT 1",
    )
    .await;
    assert_restore_refused("newer version", &bytes, OTHER_VERSION).await;
}

#[tokio::test]
async fn restore_names_an_older_backup_as_another_version() {
    // An older app's backup lacks the newest table. The version check comes
    // first, so it reads as another version's, not as a stranger's file.
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    let bytes = forged_backup(
        &src,
        dir.path(),
        "DELETE FROM _sqlx_migrations \
         WHERE version = (SELECT MAX(version) FROM _sqlx_migrations); \
         DROP TABLE budget_amount",
    )
    .await;
    assert_restore_refused("older version", &bytes, OTHER_VERSION).await;
}

#[tokio::test]
async fn restore_reads_the_version_only_from_a_plain_table() {
    // Each stand-in reports the right version, so only the check on the table
    // itself can refuse it, before its own SQL ever runs.
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    for (case, sql) in [
        (
            "a view",
            "ALTER TABLE _sqlx_migrations RENAME TO m; \
             CREATE VIEW _sqlx_migrations AS SELECT * FROM m",
        ),
        (
            "a computed version",
            "ALTER TABLE _sqlx_migrations RENAME TO m; \
             CREATE TABLE _sqlx_migrations (v INTEGER, version INTEGER AS (v)); \
             INSERT INTO _sqlx_migrations (v) SELECT version FROM m",
        ),
        (
            "a virtual table",
            "ALTER TABLE _sqlx_migrations RENAME TO m; \
             CREATE VIRTUAL TABLE _sqlx_migrations USING fts5(version); \
             INSERT INTO _sqlx_migrations SELECT version FROM m",
        ),
    ] {
        let bytes = forged_backup(&src, dir.path(), sql).await;
        assert_restore_refused(case, &bytes, NOT_A_BACKUP).await;
    }
}

#[tokio::test]
async fn restore_refuses_tables_that_arent_this_apps() {
    // Reading a view, a virtual table or a generated column runs SQL the file
    // supplies. Only the comparison of CREATE statements refuses each of these
    // as not ours: the view and the virtual table hold no rows and the copy
    // skips the generated column, so they would restore, and a missing table
    // would fail as an opaque database error.
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    for (case, sql) in [
        ("a missing table", "DROP TABLE rule_skip"),
        (
            "a view",
            "DROP TABLE rule_skip; \
             CREATE VIEW rule_skip AS \
             SELECT 1 AS rule_id, '2026-01-01' AS occurrence_date WHERE 0",
        ),
        (
            "a virtual table",
            "DROP TABLE rule_skip; \
             CREATE VIRTUAL TABLE rule_skip USING fts5(rule_id, occurrence_date)",
        ),
        (
            "a generated column",
            "ALTER TABLE category \
             ADD COLUMN shout TEXT GENERATED ALWAYS AS (upper(label)) VIRTUAL",
        ),
    ] {
        let bytes = forged_backup(&src, dir.path(), sql).await;
        assert_restore_refused(case, &bytes, NOT_A_BACKUP).await;
    }
}

#[tokio::test]
async fn restore_refuses_any_table_whose_statement_differs() {
    // A trailing space changes nothing SQLite reads, so only the comparison of
    // CREATE statements can refuse it, for each table restore reads.
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    for name in crate::backup::TABLES
        .iter()
        .map(|table| table.name)
        .chain(["sqlite_sequence"])
    {
        let sql = format!(
            "PRAGMA writable_schema = ON; \
             UPDATE sqlite_master SET sql = sql || ' ' WHERE name = '{name}'"
        );
        let bytes = forged_backup(&src, dir.path(), &sql).await;
        assert_restore_refused(name, &bytes, NOT_A_BACKUP).await;
    }
}

#[tokio::test]
async fn restore_copies_only_the_columns_this_app_declares() {
    // A field tacked on past a table's declared columns, then hidden by giving
    // the table back its own CREATE statement, must not be copied in to surface
    // in whatever column a later migration adds.
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    seed_category(&src, "Food").await;
    let bytes = forged_backup(
        &src,
        dir.path(),
        "ALTER TABLE account ADD COLUMN planted ANY; UPDATE account SET planted = 'x'; \
         ALTER TABLE category ADD COLUMN planted ANY; UPDATE category SET planted = 'x'; \
         PRAGMA writable_schema = ON; \
         UPDATE sqlite_master SET sql = replace(sql, ', planted ANY', '') \
         WHERE name IN ('account', 'category')",
    )
    .await;
    let (_dst_dir, dst) = restored(&bytes).await;
    for table in ["account", "category"] {
        // What a migration adding a column would find in it.
        sqlx::raw_sql(AssertSqlSafe(format!(
            "ALTER TABLE {table} ADD COLUMN later ANY"
        )))
        .execute(&dst)
        .await
        .unwrap();
        let later: Vec<Option<String>> =
            sqlx::query_scalar(AssertSqlSafe(format!("SELECT later FROM {table}")))
                .fetch_all(&dst)
                .await
                .unwrap();
        assert_eq!(later, [None], "{table}");
    }
}

#[tokio::test]
async fn tables_lists_every_data_table() {
    // A table a migration adds must join `TABLES`, or backups would leave it
    // behind.
    let (_dir, pool) = fixture().await;
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations' \
         ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut listed: Vec<&str> = crate::backup::TABLES
        .iter()
        .map(|table| table.name)
        .collect();
    listed.sort_unstable();
    assert_eq!(tables, listed);
}

#[tokio::test]
async fn restore_ignores_whatever_else_a_backup_holds() {
    // Only the tables restore reads are checked. Checking the whole file would
    // evaluate this table's CHECK, which the file supplies, so it would refuse
    // the row that already fails it.
    let (dir, src) = fixture().await;
    let account = seed_account(&src).await;
    let bytes = forged_backup(
        &src,
        dir.path(),
        "CREATE TABLE extra (x CHECK (x > 0)); \
         PRAGMA ignore_check_constraints = ON; \
         INSERT INTO extra VALUES (-1)",
    )
    .await;
    let (_dst_dir, dst) = restored(&bytes).await;
    assert_eq!(repo::account::list(&dst).await.unwrap(), vec![account]);
}

/// A snapshot of `pool` with the type byte of `object`'s root page wiped (a
/// table's or an index's), so SQLite finds it damaged while the schema reads
/// fine.
async fn damaged_backup(pool: &SqlitePool, dir: &Path, object: &str) -> Vec<u8> {
    let mut bytes = crate::backup::snapshot(pool, dir).await.unwrap();
    let path = dir.join("damaged.sqlite3");
    std::fs::write(&path, &bytes).unwrap();
    let options = SqliteConnectOptions::new().filename(&path).read_only(true);
    let damaged = SqlitePool::connect_with(options).await.unwrap();
    let root: i64 = sqlx::query_scalar("SELECT rootpage FROM sqlite_master WHERE name = ?")
        .bind(object)
        .fetch_one(&damaged)
        .await
        .unwrap();
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(&damaged)
        .await
        .unwrap();
    damaged.close().await;
    // No valid b-tree page has type 0.
    bytes[usize::try_from((root - 1) * page_size).unwrap()] = 0;
    bytes
}

#[tokio::test]
async fn restore_refuses_a_damaged_table() {
    // Reading it would fail as an opaque database error; checking first gives
    // the plain refusal.
    let (dir, src) = populated_source().await;
    let bytes = damaged_backup(&src, dir.path(), "entry").await;
    assert_restore_refused("damaged entry", &bytes, UNREADABLE).await;
}

#[tokio::test]
async fn restore_refuses_a_table_whose_index_is_damaged() {
    // The integrity check reports a damaged index as rows, without failing.
    let (dir, src) = populated_source().await;
    let bytes = damaged_backup(&src, dir.path(), "idx_entry_account_date").await;
    assert_restore_refused("damaged index", &bytes, UNREADABLE).await;
}

#[tokio::test]
async fn restore_refuses_a_row_the_app_cannot_read_in_any_table() {
    // One row per table that SQLite's own constraints let through, but the
    // validating constructors reject.
    let (dir, src) = populated_source().await;
    for (case, sql) in [
        (
            "account",
            "UPDATE account SET opening_balance = '79228162514264337593543950335' \
             WHERE kind = 'normal'",
        ),
        (
            "account_member",
            "UPDATE account_member SET member_account_id = summary_account_id",
        ),
        ("category", "UPDATE category SET label = hex(zeroblob(101))"),
        (
            "category (not UTF-8)",
            "UPDATE category SET label = CAST(x'ff' AS TEXT)",
        ),
        ("budget", "UPDATE budget SET amount = '-1.00'"),
        ("budget (id 0)", "UPDATE budget SET id = 0"),
        ("budget_amount", "UPDATE budget_amount SET amount = 'lots'"),
        ("recurring_rule", "UPDATE recurring_rule SET amount = '0'"),
        ("entry", "UPDATE entry SET date = '2026-02-30'"),
        ("entry (id 0)", "UPDATE entry SET id = 0"),
        ("entry (amount)", "UPDATE entry SET amount = 'lots'"),
        (
            "entry (a second spelling)",
            "UPDATE entry SET date = '+' || date",
        ),
        ("rule_amount", "UPDATE rule_amount SET amount = '-60.00'"),
        (
            "rule_skip",
            "UPDATE rule_skip SET occurrence_date = 'someday'",
        ),
        (
            "rule_skip (a second spelling)",
            "UPDATE rule_skip SET occurrence_date = '+' || occurrence_date",
        ),
    ] {
        let bytes = forged_backup(&src, dir.path(), sql).await;
        assert_restore_refused(case, &bytes, UNREADABLE).await;
    }
}

#[tokio::test]
async fn restore_refuses_an_orphaned_row() {
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    let bytes = forged_backup(
        &src,
        dir.path(),
        "INSERT INTO entry (account_id, amount, kind, date) \
         VALUES (999, '1.00', 'income', '2026-03-01')",
    )
    .await;
    assert_restore_refused("orphaned entry", &bytes, UNREADABLE).await;
}

#[tokio::test]
async fn restore_refuses_accounts_that_break_their_rules() {
    // Two USD accounts (ids 1 and 2) and a summary over both (id 3). Each case
    // breaks one rule the app keeps for accounts.
    let (dir, src) = fixture().await;
    let a = insert_account(&src, &account_with("USD", "A"))
        .await
        .unwrap();
    let b = insert_account(&src, &account_with("USD", "B"))
        .await
        .unwrap();
    insert_account(&src, &summary_over("USD", "All", vec![a.id(), b.id()]))
        .await
        .unwrap();
    for (case, sql) in [
        ("no members", "DELETE FROM account_member"),
        (
            "includes itself",
            "INSERT INTO account_member VALUES (3, 3)",
        ),
        (
            "a missing member",
            "INSERT INTO account_member VALUES (3, 99)",
        ),
        (
            "a nested summary",
            "UPDATE account SET kind = 'summary' WHERE id = 2; \
             INSERT INTO account_member VALUES (2, 1)",
        ),
        (
            "mixed currencies",
            "UPDATE account SET currency = 'EUR' WHERE id = 2",
        ),
        (
            "an entry of its own",
            "INSERT INTO entry (account_id, amount, kind, date) \
             VALUES (3, '1.00', 'income', '2026-03-01')",
        ),
        (
            "a rule of its own",
            "INSERT INTO recurring_rule \
             (account_id, amount, kind, start_date, end_kind, freq_unit, freq_interval) \
             VALUES (3, '1.00', 'expense', '2026-01-05', 'never', 'monthly', 1)",
        ),
        (
            "a budget of its own",
            "INSERT INTO category (label, icon_kind, icon_value) VALUES ('Food', 'preset', 'tag'); \
             INSERT INTO budget (account_id, category_id, amount, start_year, start_month) \
             VALUES (3, 1, '10.00', 2026, 1)",
        ),
        (
            "members under a normal account",
            "INSERT INTO account_member VALUES (1, 2)",
        ),
        (
            "a normal account listing itself",
            "INSERT INTO account_member VALUES (1, 1)",
        ),
        (
            "a summary's opening balance",
            "UPDATE account SET opening_balance = '123.45' WHERE id = 3",
        ),
        (
            "an anchor before 1969",
            "UPDATE account SET anchor_year = 1968 WHERE id = 1",
        ),
    ] {
        let bytes = forged_backup(&src, dir.path(), sql).await;
        assert_restore_refused(case, &bytes, UNREADABLE).await;
    }
}

#[tokio::test]
async fn restore_refuses_ids_out_of_range() {
    // Ids reach the frontend as JSON numbers, and at i64::MAX every later
    // insert fails. Moving an id leaves the counter alone, and the account has
    // no records, so only the id itself is at fault.
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    for (case, sql) in [
        ("negative", "UPDATE account SET id = -7"),
        ("zero", "UPDATE account SET id = 0"),
        ("past 2^52", "UPDATE account SET id = 4503599627370497"),
        ("i64::MAX", "UPDATE account SET id = 9223372036854775807"),
    ] {
        let bytes = forged_backup(&src, dir.path(), sql).await;
        assert_restore_refused(case, &bytes, UNREADABLE).await;
    }
}

#[tokio::test]
async fn restore_refuses_counters_out_of_range() {
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    for (case, sql) in [
        (
            "i64::MAX",
            "UPDATE sqlite_sequence SET seq = 9223372036854775807 WHERE name = 'account'",
        ),
        (
            "past 2^52",
            "UPDATE sqlite_sequence SET seq = 4503599627370497 WHERE name = 'account'",
        ),
        (
            "negative",
            "UPDATE sqlite_sequence SET seq = -1 WHERE name = 'account'",
        ),
        (
            "text",
            "UPDATE sqlite_sequence SET seq = '12' WHERE name = 'account'",
        ),
        (
            "a fraction",
            "UPDATE sqlite_sequence SET seq = 1.5 WHERE name = 'account'",
        ),
        (
            "a blob",
            "UPDATE sqlite_sequence SET seq = x'00' WHERE name = 'account'",
        ),
    ] {
        let bytes = forged_backup(&src, dir.path(), sql).await;
        assert_restore_refused(case, &bytes, UNREADABLE).await;
    }
}

#[tokio::test]
async fn restore_accepts_ids_and_counters_at_the_limit() {
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    let bytes = forged_backup(
        &src,
        dir.path(),
        "UPDATE account SET id = 4503599627370496; \
         UPDATE sqlite_sequence SET seq = 4503599627370496 WHERE name = 'account'",
    )
    .await;
    let (_dst_dir, dst) = restored(&bytes).await;
    let ids: Vec<u64> = repo::account::list(&dst)
        .await
        .unwrap()
        .iter()
        .map(|account| account.id().get())
        .collect();
    assert_eq!(ids, [1 << 52]);
    // The restored counter carries on from there.
    let next = insert_account(&dst, &usd_account()).await.unwrap();
    assert_eq!(next.id().get(), (1 << 52) + 1);
}

#[tokio::test]
async fn restore_refuses_more_rows_than_a_table_allows() {
    let cap = |name: &str| {
        crate::backup::TABLES
            .iter()
            .find(|table| table.name == name)
            .unwrap()
            .max_rows
    };
    let accounts = |count: i64| {
        format!(
            "WITH RECURSIVE n(i) AS (SELECT 2 UNION ALL SELECT i + 1 FROM n WHERE i < {count}) \
             INSERT INTO account (name, icon, currency, opening_balance, anchor_year, anchor_month) \
             SELECT 'A' || i, '🏦', 'USD', '0', 2026, 1 FROM n"
        )
    };
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    // As many as allowed restore...
    let at_cap = forged_backup(&src, dir.path(), &accounts(cap("account"))).await;
    let _accepted = restored(&at_cap).await;
    // ...one more doesn't, in any table counted.
    let over = forged_backup(&src, dir.path(), &accounts(cap("account") + 1)).await;
    assert_restore_refused("accounts", &over, OVERSIZED).await;
    let categories = format!(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {}) \
         INSERT INTO category (label, icon_kind, icon_value) SELECT 'C' || i, 'preset', 'tag' FROM n",
        cap("category") + 1
    );
    let over = forged_backup(&src, dir.path(), &categories).await;
    assert_restore_refused("categories", &over, OVERSIZED).await;
}

#[tokio::test]
async fn restore_counts_rows_past_the_files_own_indexes() {
    // The file's indexes are its own to fill: here every one that could answer
    // a count is pointed at an empty b-tree while the table holds one row more
    // than allowed. This SQLite's integrity check counts index entries and
    // refuses it as damaged; one that didn't would still have the rows counted
    // from the table itself (`NOT INDEXED`) and refuse it as oversized.
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    let sql = format!(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {}) \
         INSERT INTO account_member SELECT i, i FROM n; \
         CREATE TABLE decoy (x, y); \
         CREATE INDEX decoy_a ON decoy (x); CREATE INDEX decoy_b ON decoy (x); \
         CREATE INDEX decoy_c ON decoy (x, y); \
         PRAGMA writable_schema = ON; \
         UPDATE sqlite_master SET rootpage = \
           (SELECT rootpage FROM sqlite_master WHERE name = 'decoy_a') \
           WHERE name = 'idx_account_member_summary'; \
         UPDATE sqlite_master SET rootpage = \
           (SELECT rootpage FROM sqlite_master WHERE name = 'decoy_b') \
           WHERE name = 'idx_account_member_member'; \
         UPDATE sqlite_master SET rootpage = \
           (SELECT rootpage FROM sqlite_master WHERE name = 'decoy_c') \
           WHERE name = 'sqlite_autoindex_account_member_1'",
        crate::backup::TABLES
            .iter()
            .find(|table| table.name == "account_member")
            .unwrap()
            .max_rows
            + 1
    );
    let bytes = forged_backup(&src, dir.path(), &sql).await;
    let (dst_dir, dst) = fixture().await;
    let before = table_state(&dst).await;
    let result = crate::backup::restore(&dst, dst_dir.path(), &bytes).await;
    assert!(
        matches!(&result, Err(CommandError::Backup(message))
            if message.contains(UNREADABLE) || message.contains(OVERSIZED)),
        "{result:?}"
    );
    assert_eq!(table_state(&dst).await, before);
}

#[tokio::test]
async fn restore_refuses_text_longer_than_any_the_app_writes() {
    // Each would pass once trimmed, as the domain trims it, then cost its whole
    // size on every read.
    let (dir, src) = populated_source().await;
    for (case, sql) in [
        (
            "a padded amount",
            "UPDATE entry SET amount = amount || printf('%.*c', 100, ' ')",
        ),
        (
            "a padded currency",
            "UPDATE account SET currency = currency || printf('%.*c', 5000, ' ')",
        ),
        (
            "a padded budget",
            "UPDATE budget SET amount = amount || printf('%.*c', 100, ' ')",
        ),
    ] {
        let bytes = forged_backup(&src, dir.path(), sql).await;
        assert_restore_refused(case, &bytes, OVERSIZED).await;
    }
}

#[tokio::test]
async fn restore_accepts_the_longest_text_the_app_writes() {
    // At four bytes a character, the widest UTF-8 can make them.
    let (dir, src) = fixture().await;
    let account = insert_account(&src, &account_with("USD", &"🙂".repeat(200)))
        .await
        .unwrap();
    seed_category(&src, &"🙂".repeat(200)).await;
    let mut draft = new_entry(account.id(), 2_500, EntryKind::Expense);
    draft.note = Some("🙂".repeat(1_000));
    repo::entry::insert(&src, &draft.build().unwrap())
        .await
        .unwrap();
    let bytes = crate::backup::snapshot(&src, dir.path()).await.unwrap();
    let (_dst_dir, dst) = restored(&bytes).await;
    assert_eq!(table_state(&dst).await, table_state(&src).await);
}

#[tokio::test]
async fn every_text_column_has_a_considered_length_cap() {
    // Restore caps text by column (`max_text_bytes`): long for notes, labels,
    // names and icons, short for everything else. A new text column falls in
    // with the short ones unless someone decides otherwise, so a new one must
    // be added here, after checking its cap fits the longest value it holds.
    let (_dir, pool) = fixture().await;
    let mut columns = Vec::new();
    for table in crate::backup::TABLES {
        let text: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM pragma_table_info(?) WHERE upper(type) = 'TEXT' ORDER BY cid",
        )
        .bind(table.name)
        .fetch_all(&pool)
        .await
        .unwrap();
        columns.extend(
            text.into_iter()
                .map(|column| format!("{}.{column}", table.name)),
        );
    }
    assert_eq!(
        columns,
        [
            "account.name",
            "account.icon",
            "account.currency",
            "account.opening_balance",
            "account.kind",
            "category.label",
            "category.icon_kind",
            "category.icon_value",
            "budget.amount",
            "budget_amount.amount",
            "recurring_rule.amount",
            "recurring_rule.kind",
            "recurring_rule.note",
            "recurring_rule.start_date",
            "recurring_rule.end_kind",
            "recurring_rule.end_date",
            "recurring_rule.freq_unit",
            "entry.amount",
            "entry.kind",
            "entry.date",
            "entry.note",
            "rule_amount.effective_from",
            "rule_amount.amount",
            "rule_skip.occurrence_date",
        ]
    );
}

#[tokio::test]
async fn restore_holds_each_rule_and_budget_to_its_breakpoint_limit() {
    // Each is read with all its breakpoints at once, so the tables' own caps
    // aren't enough. The base amount is a segment too, so 99 is the most.
    let breakpoints = |count: i64| {
        format!(
            "DELETE FROM rule_amount; DELETE FROM budget_amount; \
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {count}) \
             INSERT INTO rule_amount (rule_id, effective_from, amount) \
             SELECT 1, date('2026-01-05', '+' || i || ' days'), '10.00' FROM n; \
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {count}) \
             INSERT INTO budget_amount (budget_id, effective_year, effective_month, amount) \
             SELECT 1, 2026 + i / 12, 1 + i % 12, '10.00' FROM n"
        )
    };
    let (dir, src) = populated_source().await;
    let most = forged_backup(&src, dir.path(), &breakpoints(99)).await;
    let _accepted = restored(&most).await;
    let over = forged_backup(&src, dir.path(), &breakpoints(100)).await;
    assert_restore_refused("100 breakpoints", &over, OVERSIZED).await;
}

#[tokio::test]
async fn restore_refuses_counters_other_than_one_per_table() {
    // Every insert scans the counters for its table's, so a flood of them
    // would slow every later insert.
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    for (case, sql) in [
        (
            "a stray name",
            "INSERT INTO sqlite_sequence VALUES ('stray', 1)",
        ),
        (
            "a duplicate",
            "INSERT INTO sqlite_sequence VALUES ('account', 1)",
        ),
        (
            "a flood",
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 50) \
             INSERT INTO sqlite_sequence SELECT 'account', 1 FROM n",
        ),
    ] {
        let bytes = forged_backup(&src, dir.path(), sql).await;
        assert_restore_refused(case, &bytes, UNREADABLE).await;
    }
}

#[tokio::test]
async fn sqlite_memory_is_capped_for_the_whole_process() {
    use sqlx::Connection as _;
    // Set on the pool's connections, it holds for any other in the process.
    let (_dir, _pool) = fixture().await;
    let mut other = sqlx::SqliteConnection::connect("sqlite::memory:")
        .await
        .unwrap();
    let limit: i64 = sqlx::query_scalar("PRAGMA hard_heap_limit")
        .fetch_one(&mut other)
        .await
        .unwrap();
    assert_eq!(limit, 256 * 1024 * 1024);
}

#[tokio::test]
async fn restore_refuses_statistics_sized_to_exhaust_memory() {
    // Attaching a file loads its `sqlite_stat4` samples, sized by the file:
    // 30,000 samples for a 500-column index ask SQLite for some 360 MB, past
    // its heap cap, before anything in the file could be checked.
    let (dir, src) = fixture().await;
    seed_account(&src).await;
    let columns = (1..=500)
        .map(|i| format!("c{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "CREATE TABLE wide ({columns}); CREATE INDEX wide_idx ON wide ({columns}); ANALYZE; \
         WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 30000) \
         INSERT INTO sqlite_stat4 (tbl, idx, neq, nlt, ndlt, sample) \
         SELECT 'wide', 'wide_idx', '1', '1', '1', x'00' FROM n"
    );
    let bytes = forged_backup(&src, dir.path(), &sql).await;
    assert_restore_refused("oversized statistics", &bytes, NOT_A_BACKUP).await;
}

#[tokio::test]
async fn restore_gives_a_file_only_so_long_to_prove_itself_ours() {
    // Enough categories that checking them takes thousands of SQLite steps, so
    // the clock is consulted before the checks are through.
    let (dir, src) = fixture().await;
    let bytes = forged_backup(
        &src,
        dir.path(),
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 400) \
         INSERT INTO category (label, icon_kind, icon_value) SELECT 'C' || i, 'preset', 'tag' FROM n",
    )
    .await;
    let (dst_dir, dst) = fixture().await;
    insert_account(&dst, &account_with("EUR", "Local"))
        .await
        .unwrap();
    let before = table_state(&dst).await;
    let out_of_time =
        crate::backup::restore_within(&dst, dst_dir.path(), &bytes, std::time::Duration::ZERO)
            .await;
    assert!(
        matches!(out_of_time, Err(CommandError::Backup(_))),
        "{out_of_time:?}"
    );
    assert_eq!(table_state(&dst).await, before);
    // Given the usual time, the same file restores, on the same pool.
    crate::backup::restore(&dst, dst_dir.path(), &bytes)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_refused_restore_leaves_restore_working() {
    // Refused before its transaction, at the table check, and after the copy
    // (rolled back): after each, a good backup must still restore on the pool.
    let (dir, src) = populated_source().await;
    let good = crate::backup::snapshot(&src, dir.path()).await.unwrap();
    let (dst_dir, dst) = fixture().await;
    for (sql, expected) in [
        (
            "UPDATE _sqlx_migrations SET version = 'abc' \
             WHERE version = (SELECT MAX(version) FROM _sqlx_migrations)",
            NOT_A_BACKUP,
        ),
        (
            "DROP TABLE rule_skip; \
             CREATE VIEW rule_skip AS SELECT 1 AS rule_id, 2 AS occurrence_date",
            NOT_A_BACKUP,
        ),
        ("UPDATE entry SET amount = '-5.00'", UNREADABLE),
    ] {
        let bad = forged_backup(&src, dir.path(), sql).await;
        let result = crate::backup::restore(&dst, dst_dir.path(), &bad).await;
        assert!(
            matches!(&result, Err(CommandError::Backup(message)) if message.contains(expected)),
            "{sql}: {result:?}"
        );
        crate::backup::restore(&dst, dst_dir.path(), &good)
            .await
            .unwrap();
    }
    assert_eq!(table_state(&dst).await, table_state(&src).await);
}

#[tokio::test]
async fn a_stored_id_that_isnt_positive_fails_to_load() {
    // Mapping it to a stand-in would alias every such row to one id.
    for id in [-7, 0] {
        let (_dir, pool) = fixture().await;
        seed_account(&pool).await;
        sqlx::query("UPDATE account SET id = ?")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            matches!(repo::account::list(&pool).await, Err(RepoError::Corrupt(_))),
            "{id}"
        );
    }
}

//! Integration tests for the persistence layer, against a real (temp-file)
//! `SQLite` database created through the production [`crate::db::init_pool`] path
//! (so migrations, `foreign_keys=ON`, and WAL are all exercised).

use sqlx::SqlitePool;
use tempfile::TempDir;
use time::{Date, Month as TMonth};

use talea_core::{
    month_summary, Account, AccountId, AccountKind, AmountSegment, Category, CategoryIcon,
    CategoryId, Currency, Entry, EntryId, EntryKind, FreqUnit, Frequency, Money, Month,
    RecurringRule, RecurringRuleId, RuleEnd,
};

use crate::db;
use crate::dto::{IsoDate, NewBudget, NewEntry, OccurrenceRef};
use crate::error::{CommandError, RepoError};
use crate::repo;

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
    repo::account::insert(pool, &usd_account()).await.unwrap()
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

    let saved = repo::account::insert(&pool, &draft).await.unwrap();
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
    assert!(rules[0].expand_in(Month::new(2026, 2).unwrap()).is_empty());
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

    assert!(repo::account::delete(&pool, account.id()).await.unwrap());
    // CASCADE only fires with foreign_keys=ON, so this also proves the pragma.
    assert!(repo::entry::for_account(&pool, account.id())
        .await
        .unwrap()
        .is_empty());
    assert!(repo::rule::for_account(&pool, account.id())
        .await
        .unwrap()
        .is_empty());
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
    let saved = repo::account::insert(&pool, &draft).await.unwrap();
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
    let to = repo::account::insert(&pool, &account_with("USD", "Savings"))
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
    let eur = repo::account::insert(&pool, &account_with("EUR", "Euro"))
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
    let other = repo::account::insert(&pool, &account_with("USD", "Other"))
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
    assert!(repo::skip::for_account(&pool, account.id())
        .await
        .unwrap()
        .is_empty());

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
    repo::account::insert(&dst, &other).await.unwrap();

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

#[tokio::test]
async fn restore_rejects_a_non_sqlite_file() {
    let (dir, pool) = fixture().await;
    let err = crate::backup::restore(&pool, dir.path(), b"not a database")
        .await
        .unwrap_err();
    assert!(matches!(err, CommandError::Backup(_)));
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
    let a = repo::account::insert(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let b = repo::account::insert(&pool, &account_with("USD", "B"))
        .await
        .unwrap();
    seed_entry(&pool, a.id(), 50_000, EntryKind::Income).await; // +500 on A
    seed_entry(&pool, b.id(), 30_000, EntryKind::Income).await; // +300 on B
    seed_entry(&pool, b.id(), 10_000, EntryKind::Expense).await; // -100 on B

    let summary = repo::account::insert(&pool, &summary_over("USD", "All", vec![a.id(), b.id()]))
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
    let a = repo::account::insert(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let b = repo::account::insert(&pool, &account_with("USD", "B"))
        .await
        .unwrap();
    seed_entry(&pool, a.id(), 10_000, EntryKind::Expense).await; // -100 (uncategorized)
    seed_entry(&pool, b.id(), 20_000, EntryKind::Expense).await; // -200 (uncategorized)

    let summary = repo::account::insert(&pool, &summary_over("USD", "All", vec![a.id(), b.id()]))
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
    let a = repo::account::insert(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let summary = repo::account::insert(&pool, &summary_over("USD", "All", vec![a.id()]))
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

#[tokio::test]
async fn summary_membership_requires_matching_currency() {
    let (_dir, pool) = fixture().await;
    let usd = repo::account::insert(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    // A EUR summary cannot take a USD member.
    let draft = summary_over("EUR", "All", vec![usd.id()]);
    assert!(matches!(
        crate::commands::validate_membership(&pool, &draft).await,
        Err(CommandError::Validation(_))
    ));
}

#[tokio::test]
async fn backup_round_trip_preserves_summary_membership() {
    let (src_dir, src) = fixture().await;
    let a = repo::account::insert(&src, &account_with("USD", "A"))
        .await
        .unwrap();
    let b = repo::account::insert(&src, &account_with("USD", "B"))
        .await
        .unwrap();
    repo::account::insert(&src, &summary_over("USD", "All", vec![a.id(), b.id()]))
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

#[tokio::test]
async fn restore_rejects_a_different_schema_version() {
    // A valid Talea snapshot...
    let (src_dir, src) = fixture().await;
    seed_account(&src).await;
    let bytes = crate::backup::snapshot(&src, src_dir.path()).await.unwrap();

    // ...forged to claim a newer schema version than this app has (other columns
    // copied from a real migration row so the table stays well-formed).
    let forged = src_dir.path().join("forged.sqlite3");
    std::fs::write(&forged, &bytes).unwrap();
    let pool = SqlitePool::connect(&format!("sqlite://{}", forged.display()))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO _sqlx_migrations \
         (version, description, installed_on, success, checksum, execution_time) \
         SELECT 9999, description, installed_on, success, checksum, execution_time \
         FROM _sqlx_migrations LIMIT 1",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
    let forged_bytes = std::fs::read(&forged).unwrap();

    // Restoring it must be refused, leaving the destination untouched.
    let (dst_dir, dst) = fixture().await;
    let err = crate::backup::restore(&dst, dst_dir.path(), &forged_bytes)
        .await
        .unwrap_err();
    assert!(matches!(err, CommandError::Backup(_)));
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
    assert!(repo::budget::for_account(&pool, account.id())
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn delete_account_cascades_its_budgets() {
    let (_dir, pool) = fixture().await;
    let account = seed_account(&pool).await;
    let groceries = seed_category(&pool, "Groceries").await;
    crate::commands::create_budget_inner(&pool, new_budget(account.id(), groceries.id(), 40_000))
        .await
        .unwrap();

    assert!(repo::account::delete(&pool, account.id()).await.unwrap());
    assert!(repo::budget::for_account(&pool, account.id())
        .await
        .unwrap()
        .is_empty());
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
    let a = repo::account::insert(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let summary = repo::account::insert(&pool, &summary_over("USD", "All", vec![a.id()]))
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
    let a = repo::account::insert(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let b = repo::account::insert(&pool, &account_with("USD", "B"))
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

    let summary = repo::account::insert(&pool, &summary_over("USD", "All", vec![a.id(), b.id()]))
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
    assert!(statuses.is_empty());

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
    let a = repo::account::insert(&pool, &account_with("USD", "A"))
        .await
        .unwrap();
    let b = repo::account::insert(&pool, &account_with("USD", "B"))
        .await
        .unwrap();
    let march = |d| date(2026, TMonth::March, d);
    seed_entry_on(&pool, a.id(), 50_000, EntryKind::Income, march(1)).await; // +500 on A
    seed_entry_on(&pool, b.id(), 10_000, EntryKind::Expense, march(2)).await; // -100 on B

    // After the cut-off, so neither member counts it yet.
    seed_entry_on(&pool, b.id(), 99_900, EntryKind::Expense, march(20)).await;
    let summary = repo::account::insert(&pool, &summary_over("USD", "All", vec![a.id(), b.id()]))
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
        members.push(repo::account::insert(&pool, &account).await.unwrap().id());
    }
    let count = i64::try_from(members.len()).unwrap();
    let summary = repo::account::insert(&pool, &summary_over("USD", "All", members))
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

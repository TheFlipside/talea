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
use crate::dto::{NewEntry, OccurrenceRef};
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
    let (_a, _e, rules) = crate::commands::load_account_data(&pool, account.id())
        .await
        .unwrap();
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

    let (loaded, entries, rules) = crate::commands::load_account_data(&pool, account.id())
        .await
        .unwrap();
    assert_eq!(loaded.id(), account.id());
    assert_eq!(entries.len(), 1);
    assert_eq!(rules.len(), 1);
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

//! The native confirmation a new backup server needs before it is saved.
//!
//! Whatever saves the backup settings decides where the whole database is
//! uploaded to and restored from, and the webview can save them. So a script
//! running there that shouldn't be, such as a compromised dependency, could
//! point backups at a server of its own. The OS draws this dialog outside the
//! webview, where no script can click it, reword it, or cover it up.
//!
//! Its text is the shell's own (`locales/native-dialogs.json`, compiled in by
//! `build.rs`), so none the webview can change at runtime. The OS's language
//! picks which, so a script can't choose one the user doesn't read.

use std::future::Future;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use tauri::AppHandle;
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};

use crate::backup::NewServer;
use crate::error::CommandError;
use crate::lock::lock;

/// The confirmation in one language.
#[derive(Debug)]
struct Wording {
    title: &'static str,
    /// What the server will be trusted with, leading into its details.
    lead: &'static str,
    /// Labels the username.
    user: &'static str,
    caution: &'static str,
    /// The only answer that saves.
    confirm: &'static str,
    cancel: &'static str,
}

// `WORDINGS`: the confirmation in each language of `locales/native-dialogs.json`,
// by language code, English first (see `build.rs`).
include!(concat!(env!("OUT_DIR"), "/dialog_wordings.rs"));

/// How long the user has to answer. A dialog the OS took away without an
/// answer would otherwise keep the save waiting, and the backup buttons
/// disabled, for good. Past it the answer is no, though the dialog stays up.
const ANSWER_TIME: Duration = Duration::from_mins(2);

/// How soon after the dialog is shown a yes counts. A script can't press the
/// button, but it can open the dialog just as the user taps where the button
/// will appear, having drawn something there to invite the tap; nobody reads
/// the dialog that fast. Timed from the request to show it, a little before it
/// appears, so it is set with room to spare.
const MIN_READ_TIME: Duration = Duration::from_millis(1_500);

/// How long a dialog may go unanswered before it is taken to be gone. The OS
/// can take one away without answering (an Android activity recreated, an iOS
/// presentation that failed); until then, its hold on the screen keeps any
/// other from opening over it. By the wall clock, as the pauses are: a phone's
/// monotonic clock stops while it sleeps.
const PRESUMED_GONE: Duration = Duration::from_mins(10);

/// How many new servers may be turned down in a row before the questions
/// pause. A script that asks again whenever it is turned down, hoping for a
/// tap at the wrong moment, gets no dialog during a pause.
const MAX_DECLINES: u32 = 3;

/// The first pause. Each decline after it doubles the next one, up to
/// [`LONGEST_PAUSE`], until a server is confirmed: a script that keeps asking
/// gets a handful of dialogs a day, while a user who mistyped a few times
/// waits minutes.
const FIRST_PAUSE: Duration = Duration::from_mins(10);

/// The longest pause.
const LONGEST_PAUSE: Duration = Duration::from_hours(24);

/// The servers turned down since the last confirmed one.
#[derive(Debug, Default)]
struct Declines {
    in_a_row: u32,
    /// The pause they imposed, if any.
    pause: Option<Pause>,
}

/// A pause in the questions: by the wall clock, since a phone's monotonic
/// clock stops while it sleeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pause {
    until: SystemTime,
    /// How long it was imposed for.
    length: Duration,
}

static DECLINES: Mutex<Declines> = Mutex::new(Declines {
    in_a_row: 0,
    pause: None,
});

/// The dialog on screen, if any, so only one ever is: a script can't stack
/// them, and a dialog left up past [`ANSWER_TIME`] still holds the screen (on
/// iOS a second couldn't open over it at all).
#[derive(Debug)]
struct Screen {
    /// How many dialogs have claimed it, numbering each.
    claims: u64,
    /// The claim holding it, and when it was made.
    held: Option<(u64, SystemTime)>,
}

static SCREEN: Mutex<Screen> = Mutex::new(Screen {
    claims: 0,
    held: None,
});

/// One dialog's hold on a [`Screen`], released when dropped, unless a later
/// dialog has taken the screen since, presuming this one gone.
struct OnScreen {
    screen: &'static Mutex<Screen>,
    claim: u64,
}

impl OnScreen {
    /// Claims `screen` at `now`, unless a dialog holds it and isn't yet
    /// [`PRESUMED_GONE`]. A clock set back since the claim counts the same as
    /// one gone forward: a small correction keeps the dialog's hold, and one
    /// further back than [`PRESUMED_GONE`] ends it rather than stretching it.
    fn claim(screen: &'static Mutex<Screen>, now: SystemTime) -> Option<Self> {
        let mut state = lock(screen);
        let held = state.held.is_some_and(|(_, since)| {
            let apart = now
                .duration_since(since)
                .unwrap_or_else(|set_back| set_back.duration());
            apart < PRESUMED_GONE
        });
        if held {
            return None;
        }
        state.claims = state.claims.wrapping_add(1);
        state.held = Some((state.claims, now));
        Some(Self {
            screen,
            claim: state.claims,
        })
    }
}

impl Drop for OnScreen {
    fn drop(&mut self) {
        let mut state = lock(self.screen);
        if state.held.is_some_and(|(claim, _)| claim == self.claim) {
            state.held = None;
        }
    }
}

/// The pause after `in_a_row` declines: none short of [`MAX_DECLINES`], then
/// [`FIRST_PAUSE`], doubling with each decline past it up to [`LONGEST_PAUSE`].
fn pause_after(in_a_row: u32) -> Option<Duration> {
    let past_limit = in_a_row.checked_sub(MAX_DECLINES)?;
    let doubling = 1_u32.checked_shl(past_limit).unwrap_or(u32::MAX);
    Some(
        FIRST_PAUSE
            .checked_mul(doubling)
            .map_or(LONGEST_PAUSE, |pause| pause.min(LONGEST_PAUSE)),
    )
}

/// Whether the declines have paused the questions at `now`. More of a pause
/// still to run than it was imposed for means the clock was set back since:
/// the pause is over then, rather than stretched by however far it went.
fn paused(declines: &Declines, now: SystemTime) -> bool {
    declines.pause.is_some_and(|pause| {
        pause
            .until
            .duration_since(now)
            .is_ok_and(|left| !left.is_zero() && left <= pause.length)
    })
}

/// Counts a decline, pausing the questions once there are enough in a row.
fn decline(declines: &mut Declines, now: SystemTime) {
    declines.in_a_row = declines.in_a_row.saturating_add(1);
    // A pause too long to represent can't be: it is at most a day.
    declines.pause = pause_after(declines.in_a_row)
        .and_then(|length| now.checked_add(length).map(|until| Pause { until, length }));
}

/// The wording for the first of `locales` (language tags, most preferred
/// first) the app speaks, or English.
fn wording_for(locales: impl IntoIterator<Item = String>) -> &'static Wording {
    let (_, wording) = locales
        .into_iter()
        .find_map(|locale| {
            let primary = locale.split(['-', '_']).next().unwrap_or_default();
            WORDINGS
                .iter()
                .find(|(language, _)| language.eq_ignore_ascii_case(primary))
        })
        .unwrap_or(&WORDINGS[0]);
    wording
}

/// The dialog's body: what the server is trusted with, then which server.
/// [`NewServer`] has already refused anything that wouldn't show as it is.
fn message(wording: &Wording, server: &NewServer) -> String {
    format!(
        "{lead}\n\n{address}\n{user}: {username}\n\n{caution}",
        lead = wording.lead,
        address = server.address(),
        user = wording.user,
        username = server.username(),
        caution = wording.caution,
    )
}

/// Whether the dialog can show `text` as it is. The Linux dialog reads its
/// body as a `printf` format, and a NUL ends any of its strings with a panic.
fn showable(text: &str) -> bool {
    !text.contains(['%', '\0'])
}

/// Whether `wording`'s buttons can tell a yes from a no: two labels, neither
/// empty, and the confirm one not a name the plugin reads as a button of its
/// own ("Ok", "Cancel", …), which would never match it. With equal labels,
/// both buttons would say yes.
fn answerable(wording: &Wording) -> bool {
    !wording.confirm.is_empty()
        && !wording.cancel.is_empty()
        && wording.confirm != wording.cancel
        && MessageDialogResult::from(wording.confirm.to_owned())
            == MessageDialogResult::Custom(wording.confirm.to_owned())
}

/// Asks the user, in a native dialog, whether backups may go to `server`.
/// Only the confirm button says yes, and only once the dialog has been up for
/// [`MIN_READ_TIME`]: dismissing it, or leaving it unanswered for
/// [`ANSWER_TIME`], says no.
///
/// # Errors
/// [`CommandError::Backup`] if a dialog is already open or can't be shown, the
/// confirm button was pressed too soon to have been read, or the questions
/// are paused after too many declines in a row.
pub(crate) async fn confirm_server(
    app: &AppHandle,
    server: NewServer,
) -> Result<bool, CommandError> {
    one_question(&DECLINES, || ask(app, &server)).await
}

/// Runs `ask` unless `declines` have paused the questions, and keeps count of
/// them. A question that couldn't be asked is no decline; a confirm pressed
/// too soon is, so a script luring taps gets no more tries than one turned
/// down, and the user is told why.
async fn one_question<F, Fut>(declines: &Mutex<Declines>, ask: F) -> Result<bool, CommandError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Verdict, CommandError>>,
{
    if paused(&lock(declines), SystemTime::now()) {
        return Err(CommandError::Backup(
            "Too many new servers were turned down. Try again later, or restart Talea.".into(),
        ));
    }
    let verdict = ask().await?;
    let mut declines = lock(declines);
    match verdict {
        Verdict::Yes => {
            *declines = Declines::default();
            Ok(true)
        }
        Verdict::No => {
            decline(&mut declines, SystemTime::now());
            Ok(false)
        }
        Verdict::TooSoon => {
            decline(&mut declines, SystemTime::now());
            let next = if declines.pause.is_some() {
                "Too many new servers were turned down, so try again later, or restart Talea."
            } else {
                "Save again, and read the confirmation before answering it."
            };
            Err(CommandError::Backup(format!(
                "Not saved: the server was confirmed too soon to have been read. {next}"
            )))
        }
    }
}

fn not_shown() -> CommandError {
    CommandError::Backup("Couldn't show the confirmation.".into())
}

/// Shows the dialog and waits, up to [`ANSWER_TIME`], for its answer.
///
/// # Errors
/// [`CommandError::Backup`] if a dialog is still on screen, or this one
/// couldn't be shown.
async fn ask(app: &AppHandle, server: &NewServer) -> Result<Verdict, CommandError> {
    let wording = wording_for(sys_locale::get_locales());
    let body = message(wording, server);
    let texts = [
        body.as_str(),
        wording.title,
        wording.confirm,
        wording.cancel,
    ];
    if !answerable(wording) || !texts.into_iter().all(showable) {
        return Err(not_shown());
    }
    // Claimed last, so nothing returns between the claim and the dialog: its
    // callback, or the plugin dropping it, is what gives the screen back.
    let on_screen = OnScreen::claim(&SCREEN, SystemTime::now()).ok_or_else(|| {
        CommandError::Backup(
            "Answer the open confirmation first, or try again in a few minutes.".into(),
        )
    })?;
    let (answer, answered) = tokio::sync::oneshot::channel();
    let shown_at = Instant::now();
    app.dialog()
        .message(body)
        .title(wording.title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            wording.confirm.to_owned(),
            wording.cancel.to_owned(),
        ))
        .show_with_result(move |result| {
            // The dialog is gone, so another may open.
            drop(on_screen);
            // Fails only once nothing waits for the answer any more.
            let _ = answer.send((result, shown_at.elapsed()));
        });
    let outcome = tokio::time::timeout(ANSWER_TIME, answered).await;
    verdict(answer_from(outcome), wording.confirm)
}

/// How the wait for the dialog's answer came out: past [`ANSWER_TIME`]
/// (`Err`), the callback dropped uncalled (`Ok(Err)`), or answered.
fn answer_from<Late, Dropped>(
    outcome: Result<Result<(MessageDialogResult, Duration), Dropped>, Late>,
) -> Answer {
    match outcome {
        Err(_late) => Answer::Unanswered,
        Ok(Err(_dropped)) => Answer::NotShown,
        Ok(Ok((pressed, after))) => Answer::Pressed(pressed, after),
    }
}

/// How a dialog ended.
#[derive(Debug)]
enum Answer {
    /// The button pressed, and how long after the dialog was shown. On
    /// mobile the plugin also reports a dialog it couldn't show as a press of
    /// Cancel, so there that reads as a no.
    Pressed(MessageDialogResult, Duration),
    /// Nothing within [`ANSWER_TIME`]. The dialog stays up, holding the screen.
    Unanswered,
    /// The plugin dropped the callback without calling it: never shown.
    NotShown,
}

/// What an answer says.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Yes,
    No,
    /// The confirm button, pressed sooner than [`MIN_READ_TIME`].
    TooSoon,
}

/// What `answer` says, where `confirm` labels the confirm button.
///
/// # Errors
/// [`CommandError::Backup`] if the dialog was never shown.
fn verdict(answer: Answer, confirm: &str) -> Result<Verdict, CommandError> {
    match answer {
        // The plugin names a custom button by its label, on every platform.
        Answer::Pressed(MessageDialogResult::Custom(label), after) if label == confirm => {
            Ok(if after >= MIN_READ_TIME {
                Verdict::Yes
            } else {
                Verdict::TooSoon
            })
        }
        Answer::Pressed(..) | Answer::Unanswered => Ok(Verdict::No),
        Answer::NotShown => Err(not_shown()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, SystemTime};

    use tauri_plugin_dialog::MessageDialogResult;

    use super::{
        answer_from, answerable, decline, message, one_question, pause_after, paused, showable,
        verdict, wording_for, Answer, Declines, OnScreen, Pause, Screen, Verdict, Wording,
        FIRST_PAUSE, LONGEST_PAUSE, MAX_DECLINES, MIN_READ_TIME, PRESUMED_GONE, WORDINGS,
    };
    use crate::backup::{change, Change, Destination, NewServer};
    use crate::error::CommandError;

    fn server(base_url: &str, username: &str) -> NewServer {
        match change(&Destination::default(), base_url, username).unwrap() {
            Change::Move(server) => server,
            other => panic!("not a new server: {other:?}"),
        }
    }

    fn locales(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|&tag| tag.to_owned()).collect()
    }

    fn later(time: SystemTime, by: Duration) -> SystemTime {
        time.checked_add(by).unwrap()
    }

    #[test]
    fn every_locale_words_the_dialog_so_it_can_be_shown() {
        let server = server("https://cloud.example.com/nextcloud", "o'brien");
        for (language, wording) in WORDINGS {
            assert!(answerable(wording), "{language}");
            let body = message(wording, &server);
            for text in [
                body.as_str(),
                wording.title,
                wording.confirm,
                wording.cancel,
            ] {
                assert!(showable(text), "{language}: {text}");
            }
        }
    }

    #[test]
    fn english_comes_first_to_stand_in_for_the_rest() {
        assert_eq!(WORDINGS[0].0, "en");
        assert!(WORDINGS.len() > 1);
    }

    #[test]
    fn a_wording_whose_buttons_cant_be_told_apart_is_not_shown() {
        let english = &WORDINGS[0].1;
        // Mobile reports the pressed button by its label, which the plugin
        // reads as a built-in button first: a confirm label read that way
        // would never match. Equal labels would make both buttons a yes.
        for (confirm, cancel) in [
            ("Ok", "Cancel"),
            ("Yes", "No"),
            ("Go", "Go"),
            ("", "Cancel"),
            ("Go", ""),
        ] {
            let wording = Wording {
                confirm,
                cancel,
                ..*english
            };
            assert!(!answerable(&wording), "{confirm:?} / {cancel:?}");
        }
    }

    #[test]
    fn nothing_the_linux_dialog_reads_as_formatting_is_shown() {
        assert!(showable("Use this server"));
        assert!(!showable("100% sure"));
        assert!(!showable("max\0"));
    }

    #[test]
    fn the_first_language_the_app_speaks_wins() {
        let title = |tags: &[&str]| wording_for(locales(tags)).title;
        let worded = |language: &str| {
            WORDINGS
                .iter()
                .find(|(code, _)| *code == language)
                .unwrap()
                .1
                .title
        };
        assert_eq!(title(&["de"]), worded("de"));
        assert_eq!(title(&["de-AT"]), worded("de"));
        assert_eq!(title(&["DE_at"]), worded("de"));
        assert_eq!(title(&["sv-SE", "de", "fr"]), worded("de"));
        for unknown in [&[][..], &["sv"], &[""], &["-"], &["deu"]] {
            assert_eq!(title(unknown), worded("en"), "{unknown:?}");
        }
    }

    #[test]
    fn the_message_names_the_address_and_the_user() {
        assert_eq!(
            message(
                wording_for(locales(&["en"])),
                &server("https://cloud.example.com/nextcloud", "max")
            ),
            "Talea will upload all your data to this server, and a restore will replace it \
             with what is stored there:\n\nhttps://cloud.example.com/nextcloud\nUser: max\n\n\
             Only continue if you entered this server yourself."
        );
    }

    #[test]
    fn only_the_confirm_button_read_in_time_says_yes() {
        let read = MIN_READ_TIME + Duration::from_millis(500);
        let too_soon = MIN_READ_TIME.checked_sub(Duration::from_millis(1)).unwrap();
        let pressed = |label: &str, after: Duration| {
            verdict(
                Answer::Pressed(MessageDialogResult::Custom(label.to_owned()), after),
                "Use this server",
            )
            .unwrap()
        };
        assert_eq!(pressed("Use this server", read), Verdict::Yes);
        assert_eq!(pressed("Use this server", MIN_READ_TIME), Verdict::Yes);
        // Tapped before anyone could have read it: a tap the dialog landed under.
        assert_eq!(pressed("Use this server", too_soon), Verdict::TooSoon);
        assert_eq!(pressed("Cancel", read), Verdict::No);
        assert_eq!(pressed("use this server", read), Verdict::No);
        for built_in in [
            MessageDialogResult::Ok,
            MessageDialogResult::Yes,
            MessageDialogResult::No,
            MessageDialogResult::Cancel,
        ] {
            let answer = Answer::Pressed(built_in, read);
            assert_eq!(verdict(answer, "Use this server").unwrap(), Verdict::No);
        }
        // Unanswered in time is a no; never shown is an error.
        assert_eq!(
            verdict(Answer::Unanswered, "Use this server").unwrap(),
            Verdict::No
        );
        assert!(verdict(Answer::NotShown, "Use this server").is_err());
    }

    #[test]
    fn a_late_answer_is_a_no_and_a_lost_one_an_error() {
        let read = MIN_READ_TIME + Duration::from_millis(500);
        // Past the answer time: counted as a decline, so a script letting
        // dialogs lapse still runs into the pauses.
        let late = answer_from::<(), ()>(Err(()));
        assert_eq!(verdict(late, "Use this server").unwrap(), Verdict::No);
        // The callback dropped uncalled: never shown, an error.
        let dropped = answer_from::<(), ()>(Ok(Err(())));
        assert!(verdict(dropped, "Use this server").is_err());
        let pressed = answer_from::<(), ()>(Ok(Ok((
            MessageDialogResult::Custom("Use this server".to_owned()),
            read,
        ))));
        assert_eq!(verdict(pressed, "Use this server").unwrap(), Verdict::Yes);
    }

    #[test]
    fn the_webview_is_denied_the_dialog_plugin() {
        // Only the shell may raise a native dialog: one the webview raised
        // could be shown over the confirmation to steer a tap.
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/capabilities");
        let mut denied = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let capability: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for permission in capability["permissions"].as_array().unwrap() {
                // A permission is named outright, or by an object's identifier.
                let permission = permission
                    .as_str()
                    .or_else(|| permission["identifier"].as_str())
                    .unwrap_or_else(|| panic!("{}: {permission}", path.display()));
                if permission.starts_with("dialog:") {
                    assert!(
                        permission.starts_with("dialog:deny-"),
                        "{}: {permission}",
                        path.display()
                    );
                    denied.push(permission.to_owned());
                }
            }
        }
        denied.sort();
        assert_eq!(
            denied,
            [
                "dialog:deny-message",
                "dialog:deny-open",
                "dialog:deny-save"
            ]
        );
    }

    #[test]
    fn only_one_dialog_holds_the_screen() {
        static SCREEN: Mutex<Screen> = Mutex::new(Screen {
            claims: 0,
            held: None,
        });
        let now = SystemTime::now();
        let first = OnScreen::claim(&SCREEN, now).unwrap();
        assert!(OnScreen::claim(&SCREEN, later(now, PRESUMED_GONE / 2)).is_none());
        drop(first);
        assert!(OnScreen::claim(&SCREEN, now).is_some());
    }

    #[test]
    fn a_dialog_unanswered_long_enough_is_presumed_gone() {
        static SCREEN: Mutex<Screen> = Mutex::new(Screen {
            claims: 0,
            held: None,
        });
        let now = SystemTime::now();
        let stale = OnScreen::claim(&SCREEN, now).unwrap();
        let fresh = OnScreen::claim(&SCREEN, later(now, PRESUMED_GONE)).unwrap();
        // The stale dialog's answer, arriving at last, doesn't free the screen
        // the fresh one holds.
        drop(stale);
        assert!(OnScreen::claim(&SCREEN, later(now, PRESUMED_GONE)).is_none());
        drop(fresh);
        assert!(OnScreen::claim(&SCREEN, later(now, PRESUMED_GONE)).is_some());
    }

    #[tokio::test]
    async fn a_question_that_couldnt_be_asked_is_no_decline() {
        let declines = Mutex::new(Declines::default());
        for _ in 0..=MAX_DECLINES {
            let failed = one_question(&declines, || async {
                Err::<Verdict, _>(CommandError::Backup("not shown".into()))
            })
            .await;
            assert!(failed.is_err());
        }
        assert_eq!(declines.lock().unwrap().in_a_row, 0);
        assert!(one_question(&declines, || async { Ok(Verdict::Yes) })
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn a_confirm_too_soon_is_refused_and_counted() {
        let declines = Mutex::new(Declines::default());
        let refused = one_question(&declines, || async { Ok(Verdict::TooSoon) }).await;
        assert!(
            matches!(&refused, Err(CommandError::Backup(message)) if message.contains("too soon")),
            "{refused:?}"
        );
        assert_eq!(declines.lock().unwrap().in_a_row, 1);
        // Once it is the decline that pauses the questions, it says so.
        for _ in 1..MAX_DECLINES - 1 {
            let _ = one_question(&declines, || async { Ok(Verdict::No) }).await;
        }
        let pausing = one_question(&declines, || async { Ok(Verdict::TooSoon) }).await;
        assert!(
            matches!(&pausing, Err(CommandError::Backup(message)) if message.contains("try again later")),
            "{pausing:?}"
        );
    }

    #[tokio::test]
    async fn the_questions_pause_once_enough_were_turned_down() {
        let declines = Mutex::new(Declines::default());
        for _ in 0..MAX_DECLINES {
            assert!(!one_question(&declines, || async { Ok(Verdict::No) })
                .await
                .unwrap());
        }
        let asked = AtomicBool::new(false);
        let refused = one_question(&declines, || async {
            asked.store(true, Ordering::Relaxed);
            Ok(Verdict::Yes)
        })
        .await;
        assert!(refused.is_err());
        assert!(!asked.load(Ordering::Relaxed));
    }

    #[test]
    fn each_decline_past_a_pause_lengthens_the_next() {
        let now = SystemTime::now();
        let mut declines = Declines {
            in_a_row: MAX_DECLINES,
            pause: None,
        };
        decline(&mut declines, now);
        let pause = declines.pause.unwrap();
        assert_eq!(pause.length, FIRST_PAUSE * 2);
        assert_eq!(pause.until.duration_since(now).unwrap(), FIRST_PAUSE * 2);
        assert!(paused(&declines, now));
    }

    #[test]
    fn a_clock_set_back_doesnt_stretch_the_pause() {
        let now = SystemTime::now();
        let declines = Declines {
            in_a_row: MAX_DECLINES,
            pause: Some(Pause {
                until: later(now, FIRST_PAUSE),
                length: FIRST_PAUSE,
            }),
        };
        assert!(paused(&declines, now));
        assert!(!paused(&declines, later(now, FIRST_PAUSE)));
        assert!(!paused(
            &declines,
            later(now, FIRST_PAUSE + Duration::from_secs(1))
        ));
        // The clock goes back further than the pause is long, by an hour or
        // by days: more pause left than it was imposed for, so it's over.
        for back in [Duration::from_hours(1), Duration::from_hours(2 * 24)] {
            let set_back = now.checked_sub(back).unwrap();
            assert!(!paused(&declines, set_back), "{back:?}");
        }
    }

    #[test]
    fn a_dialog_claimed_later_than_now_is_presumed_gone() {
        static SCREEN: Mutex<Screen> = Mutex::new(Screen {
            claims: 0,
            held: None,
        });
        let now = SystemTime::now();
        let _held = OnScreen::claim(&SCREEN, now).unwrap();
        // A small correction of the clock keeps the dialog's hold...
        let corrected = now.checked_sub(Duration::from_secs(1)).unwrap();
        assert!(OnScreen::claim(&SCREEN, corrected).is_none());
        // ...one further back than a dialog lasts ends it.
        let set_back = now.checked_sub(PRESUMED_GONE).unwrap();
        assert!(OnScreen::claim(&SCREEN, set_back).is_some());
    }

    #[tokio::test]
    async fn a_confirmed_server_ends_the_pauses() {
        let declines = Mutex::new(Declines {
            in_a_row: MAX_DECLINES + 5,
            pause: None,
        });
        assert!(one_question(&declines, || async { Ok(Verdict::Yes) })
            .await
            .unwrap());
        assert_eq!(declines.lock().unwrap().in_a_row, 0);
        assert!(!one_question(&declines, || async { Ok(Verdict::No) })
            .await
            .unwrap());
        assert_eq!(declines.lock().unwrap().pause, None);
    }

    #[test]
    fn the_pause_doubles_up_to_a_day() {
        assert_eq!(pause_after(MAX_DECLINES - 1), None);
        assert_eq!(pause_after(MAX_DECLINES), Some(FIRST_PAUSE));
        assert_eq!(pause_after(MAX_DECLINES + 1), Some(FIRST_PAUSE * 2));
        assert_eq!(pause_after(MAX_DECLINES + 2), Some(FIRST_PAUSE * 4));
        for in_a_row in [MAX_DECLINES + 8, MAX_DECLINES + 40, u32::MAX] {
            assert_eq!(pause_after(in_a_row), Some(LONGEST_PAUSE), "{in_a_row}");
        }
    }
}

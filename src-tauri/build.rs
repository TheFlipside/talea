//! Builds the Tauri shell, and compiles in the wording of its native dialogs.

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    native_dialog_wordings();
    tauri_build::build();
}

/// The shell's own text: the native dialogs, in each language the app speaks.
const WORDINGS_FILE: &str = "locales/native-dialogs.json";

/// The strings of the new-backup-server confirmation, as `confirm.rs`'s
/// `Wording` names them.
const FIELDS: [&str; 6] = ["title", "lead", "user", "caution", "confirm", "cancel"];

/// Whether `code` is a language code as the app names its locales: two or
/// three lowercase ASCII letters.
fn is_language_code(code: &str) -> bool {
    (2..=3).contains(&code.len()) && code.bytes().all(|byte| byte.is_ascii_lowercase())
}

/// Writes `dialog_wordings.rs` into `OUT_DIR`: a `WORDINGS` list holding the
/// new-backup-server confirmation in each language of [`WORDINGS_FILE`],
/// English first. They are fixed when the shell is built, and a language
/// missing a string fails the build.
fn native_dialog_wordings() {
    println!("cargo:rerun-if-changed={WORDINGS_FILE}");
    let text = std::fs::read_to_string(WORDINGS_FILE).expect("the native dialogs' wording");
    let file: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|err| panic!("{WORDINGS_FILE} isn't valid JSON: {err}"));
    let languages = file["newBackupServer"]
        .as_object()
        .unwrap_or_else(|| panic!("{WORDINGS_FILE} has no newBackupServer"));
    // English first: it stands in for a language the app doesn't speak.
    let mut codes: Vec<&String> = languages.keys().collect();
    codes.sort_by_key(|code| (code.as_str() != "en", code.as_str()));
    assert!(
        codes.first().is_some_and(|code| code.as_str() == "en"),
        "{WORDINGS_FILE} must word the dialogs in English, the fallback"
    );
    let mut code = String::from("const WORDINGS: &[(&str, Wording)] = &[\n");
    for language in codes {
        assert!(
            is_language_code(language),
            "{WORDINGS_FILE}: {language:?} isn't a language code"
        );
        write!(code, "    ({language:?}, Wording {{").expect("a String takes any text");
        for field in FIELDS {
            let wording = languages[language][field]
                .as_str()
                .unwrap_or_else(|| panic!("{WORDINGS_FILE}: {language} has no {field}"));
            // `{:?}` writes a string as a Rust literal, escapes included.
            write!(code, " {field}: {wording:?},").expect("a String takes any text");
        }
        code.push_str(" }),\n");
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    std::fs::write(out.join("dialog_wordings.rs"), code).expect("OUT_DIR is writable");
}

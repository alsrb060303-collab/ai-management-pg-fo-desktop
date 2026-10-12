//! The language of messages made in Rust (errors, settings-map notes, the
//! tray menu). The frontend picks it (OS language or the user's choice) and
//! sends it with `set_locale`; until then messages are Korean.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

/// `tr!("한국어 {x}", "English {x}")` formats the string for the current
/// language (both take the same `format!` arguments).
#[macro_export]
macro_rules! tr {
    ($ko:literal, $en:literal $(,)?) => {
        if $crate::i18n::english() { format!($en) } else { format!($ko) }
    };
    ($ko:literal, $en:literal, $($arg:tt)*) => {
        if $crate::i18n::english() { format!($en, $($arg)*) } else { format!($ko, $($arg)*) }
    };
}

#[tauri::command]
pub fn set_locale(app: AppHandle, locale: String) {
    let english = locale != "ko";
    if ENGLISH.swap(english, Ordering::Relaxed) != english {
        crate::tray::relabel(&app);
        crate::bridge::changed();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tr_picks_the_language_and_formats() {
        let n = 3;
        // Tests run in Korean; the English arm is only formatted when chosen.
        assert_eq!(tr!("{n}개", "{n} items"), "3개");
        assert_eq!(tr!("{}개", "{} items", n + 1), "4개");
    }
}

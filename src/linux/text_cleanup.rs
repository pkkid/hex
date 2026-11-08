//! Labs › Transform typed text on Linux: the shortcut selects and copies the
//! focused field's text (Ctrl+A, Ctrl+C), runs the focused app's Output steps
//! over it (transformations read `context.source` as `"cleanup"`), copies the
//! still-selected text again (Ctrl+C alone) to make sure nobody touched it,
//! and pastes the result over the selection. The clipboard comes back afterwards where the desktop
//! lets Hex read it, as after a dictation.

use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

use color_eyre::eyre::{Result, eyre};

use crate::dictation::modes::ModeRuntime;
use crate::linux::paste::{Letter, LinuxPaster};
use crate::personal_commands::TransformationClient;

/// How long a copy has to reach the clipboard.
const COPY_TIMEOUT: Duration = Duration::from_millis(600);
const POLL: Duration = Duration::from_millis(40);
/// Lets the app act on Ctrl+A before Ctrl+C arrives.
const KEY_GAP: Duration = Duration::from_millis(30);
/// The most text it takes on, in characters.
const MAX_CHARS: usize = 20_000;

/// What a cleanup did.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Cleaned {
    /// The steps changed the text, and the result was pasted.
    Replaced,
    /// The steps returned the text as it was; nothing was pasted.
    Unchanged,
}

/// Runs the Output steps over the focused field. An error leaves the field
/// as it was and says why.
pub(crate) fn clean_up(
    paster: &mut LinuxPaster,
    modes: &ModeRuntime,
    transformations: &TransformationClient,
    stop: &AtomicBool,
) -> Result<Cleaned> {
    let original = paster.snapshot();
    // Set once the person copied something else meanwhile: Hex then leaves
    // their clipboard as it is.
    let mut theirs = false;
    let cleaned = (|| {
        let text = copy(paster, true)?;
        check_input(&text)?;
        let context = crate::linux::context::capture(modes.reads_page());
        let processed = modes.process_typed(&text, &context, transformations, stop);
        if let Some(reason) = processed
            .observation
            .as_ref()
            .and_then(|observation| observation.fallback.as_ref())
        {
            return Err(eyre!("a step failed: {reason}"));
        }
        let cleaned = keep_outer_whitespace(&text, &processed.text);
        if cleaned == text {
            return Ok(None);
        }
        if cleaned.trim().is_empty() {
            return Err(eyre!("the steps returned nothing"));
        }
        // Something else on the clipboard means the person copied meanwhile;
        // copying again would empty it.
        if paster.text()? != text {
            theirs = true;
            return Err(eyre!(
                "the clipboard changed while the text was being transformed"
            ));
        }
        // The steps can take seconds. Copying the selection again, without
        // selecting all, matches only if nobody typed, clicked, or moved to
        // another window since.
        if !copy(paster, false).is_ok_and(|again| again == text) {
            return Err(eyre!("the text changed while it was being transformed"));
        }
        Ok(Some(cleaned))
    })();
    match cleaned {
        Ok(Some(cleaned)) => {
            paster.paste_over(&cleaned, original)?;
            Ok(Cleaned::Replaced)
        }
        result => {
            if let Some(original) = original.filter(|_| !theirs) {
                paster.restore(original);
            }
            result.map(|_| Cleaned::Unchanged)
        }
    }
}

/// Copies the focused field's selection, first selecting everything when
/// `select_all`: the clipboard is emptied first, so text that arrives is the
/// field's.
fn copy(paster: &mut LinuxPaster, select_all: bool) -> Result<String> {
    paster.clear()?;
    if select_all {
        paster.press(Letter::A)?;
        thread::sleep(KEY_GAP);
    }
    paster.press(Letter::C)?;
    let began = Instant::now();
    loop {
        thread::sleep(POLL);
        let text = paster.text()?;
        if !text.is_empty() {
            return Ok(text);
        }
        if began.elapsed() >= COPY_TIMEOUT {
            return Err(eyre!(
                "nothing was copied; click into a text field and try again"
            ));
        }
    }
}

/// Text it leaves alone: none, or too much.
fn check_input(text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Err(eyre!("the field is empty"));
    }
    let chars = text.chars().count();
    if chars > MAX_CHARS {
        return Err(eyre!(
            "the text is {chars} characters, over the {MAX_CHARS} character limit"
        ));
    }
    Ok(())
}

/// `cleaned` with `original`'s leading and trailing whitespace: steps tend
/// to trim, which would join the field's text to what follows it.
fn keep_outer_whitespace(original: &str, cleaned: &str) -> String {
    let body = cleaned.trim();
    if body.is_empty() {
        return String::new();
    }
    let leading = &original[..original.len() - original.trim_start().len()];
    let trailing = &original[original.trim_end().len()..];
    format!("{leading}{body}{trailing}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fields_outer_whitespace_survives_a_trimming_step() {
        assert_eq!(
            keep_outer_whitespace("  teh cat\n", "The cat."),
            "  The cat.\n"
        );
        assert_eq!(keep_outer_whitespace("a", " b "), "b");
        assert_eq!(keep_outer_whitespace("a\n", "  "), "");
    }

    #[test]
    fn empty_and_long_text_is_left_alone() {
        assert!(check_input("  \n").is_err());
        assert!(check_input(&"a".repeat(MAX_CHARS + 1)).is_err());
        assert!(check_input("helo").is_ok());
    }
}

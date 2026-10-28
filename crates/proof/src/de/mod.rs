//! German proofing (fork SHP-ART).
//!
//! - **Spelling** against the Hunspell German dictionary (LibreOffice's `de_DE_frami`, derived
//!   from igerman98; GPL, not bundled), found by [`crate::hunspell`] on the desktop. The web
//!   version has no folders to search, so its shell fetches the files from the office server and
//!   hands them over with [`install_dictionary_bytes`]. Compounds and the Fugen-s come from the
//!   dictionary's compound rules. Without a dictionary German text gets no spelling marks.
//! - Checking and **suggestions** use `spellbook`, a full Hunspell port: the in-house `affix`
//!   engine has no compound rules or `CHECKSHARPS`, which German needs (`Bremsflüssigkeitsprüfung`,
//!   `STRASSE`).
//! - No German grammar rules yet.

#[cfg(test)]
pub(crate) mod test_dict;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

use spellbook::Dictionary;

/// The German dictionary: `None` until first use, then the installed one (`Some(None)`: none).
static DICT: RwLock<Option<Option<Arc<Dictionary>>>> = RwLock::new(None);
static RELOAD_GENERATION: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
type TestLoadGate = (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>, Option<Arc<Dictionary>>);
#[cfg(test)]
static TEST_LOAD_GATE: std::sync::Mutex<Option<TestLoadGate>> = std::sync::Mutex::new(None);

/// File names of a German Hunspell dictionary (LibreOffice's name first).
const NAMES: &[&str] = &["de_DE_frami", "de_DE", "de-DE", "de_AT_frami", "de_CH_frami", "de"];

/// Largest `.aff` or `.dic` file read (de_DE_frami's `.dic` is about 4 MB).
pub const MAX_FILE_BYTES: u64 = 64 << 20;

/// The German dictionary: the installed Hunspell one (read on first use), one given with
/// [`install_dictionary`], or `None` when none is available (German spelling is off then).
pub fn dictionary() -> Option<Arc<Dictionary>> {
    loop {
        if let Some(d) = DICT.read().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
            return d.clone();
        }
        let generation = RELOAD_GENERATION.load(Ordering::Relaxed);
        let loaded = load_installed().map(Arc::new);
        #[cfg(test)]
        let mut loaded = loaded;
        #[cfg(test)]
        if let Some((loaded_tx, resume_rx, replacement)) = TEST_LOAD_GATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
            if let Some(replacement) = replacement {
                loaded = Some(replacement);
            }
            let _ = loaded_tx.send(());
            let _ = resume_rx.recv();
        }
        let mut slot = DICT.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        if generation != RELOAD_GENERATION.load(Ordering::Relaxed) {
            continue;
        }
        return slot.get_or_insert(loaded).clone();
    }
}

fn load_installed() -> Option<Dictionary> {
    let Some((aff, dic)) = crate::hunspell::find(NAMES) else {
        crate::hunspell::clear_load_error(NAMES[0]);
        log::info!("German spelling: no Hunspell de_DE dictionary installed");
        return None;
    };
    crate::hunspell::note_load(NAMES[0], &dic, load_files(&aff, &dic)).map_err(|e| log::warn!("German spelling: {e}")).ok()
}

/// Read a Hunspell pair from disk (size-capped), decoded per the `.aff`'s `SET` line.
pub fn load_files(aff: &Path, dic: &Path) -> Result<Dictionary, String> {
    let read = |p: &Path| -> Result<Vec<u8>, String> {
        let len = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?.len();
        if len > MAX_FILE_BYTES {
            return Err(format!("{}: too large ({len} bytes)", p.display()));
        }
        std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))
    };
    parse_bytes(&read(aff)?, &read(dic)?)
}

/// Decode per the `SET` line, then parse; spellbook reads UTF-8 only, so the `SET` line is
/// rewritten after decoding.
fn parse_bytes(aff: &[u8], dic: &[u8]) -> Result<Dictionary, String> {
    let (aff, dic) = crate::hunspell::decode(aff, dic)?;
    let aff: String = aff.lines().map(|l| if l.trim_start().starts_with("SET ") { "SET UTF-8" } else { l }).collect::<Vec<_>>().join("\n");
    Dictionary::new(&aff, &dic).map_err(|e| format!("German dictionary: {e}"))
}

/// Use this German dictionary from now on (Hunspell `.aff` and `.dic` text, decoded to UTF-8).
/// On error the current dictionary stays. Text is checked again.
pub fn install_dictionary(aff: &str, dic: &str) -> Result<(), String> {
    install_dictionary_bytes(aff.as_bytes(), dic.as_bytes())
}

/// [`install_dictionary`] from the raw files, decoded per the `.aff`'s `SET` line (the web shell
/// fetches `de_DE_frami`, which is ISO 8859-1).
pub fn install_dictionary_bytes(aff: &[u8], dic: &[u8]) -> Result<(), String> {
    for (name, bytes) in [(".aff", aff), (".dic", dic)] {
        if bytes.len() > MAX_FILE_BYTES as usize {
            return Err(format!("{name}: too large ({} bytes)", bytes.len()));
        }
    }
    let d = parse_bytes(aff, dic)?;
    *DICT.write().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Some(Arc::new(d)));
    crate::dictionaries_changed();
    Ok(())
}

/// Forget the loaded dictionary, so the next check looks for an installed one again.
pub fn reload_dictionary() {
    let mut slot = DICT.write().unwrap_or_else(std::sync::PoisonError::into_inner);
    RELOAD_GENERATION.fetch_add(1, Ordering::Relaxed);
    *slot = None;
}

/// The `.dic` file of the German dictionary found now; `None` when there is none.
pub fn installed_path() -> Option<std::path::PathBuf> {
    crate::hunspell::find(NAMES).map(|(_, dic)| dic)
}

/// Is a German dictionary available (spelling is checked)?
pub fn has_dictionary() -> bool {
    dictionary().is_some()
}

/// Is `word` spelled correctly in German? Numbers, single letters, short words in capitals
/// (`KFZ`, `HWK`), words with digits and words in other scripts are accepted, and so is every
/// word when no German dictionary is installed.
pub fn is_correct(word: &str) -> bool {
    is_correct_with(dictionary().as_deref(), word)
}

/// [`is_correct`] with the File › Options › Proofing choices.
pub fn is_correct_with_options(word: &str, opts: &crate::Options) -> bool {
    check(dictionary().as_deref(), word, opts)
}

/// [`is_correct`] against a given dictionary (`None` accepts everything).
pub fn is_correct_with(d: Option<&Dictionary>, word: &str) -> bool {
    check(d, word, &crate::Options::default())
}

fn is_latin_letter(c: char) -> bool {
    c.is_alphabetic() && (c < '\u{0250}' || ('\u{1E00}'..='\u{1EFF}').contains(&c))
}

fn check(d: Option<&Dictionary>, word: &str, opts: &crate::Options) -> bool {
    let Some(d) = d else { return true };
    let w = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’');
    let w = w.trim_matches(['\'', '’']);
    let n = w.chars().count();
    if n <= 1 || (opts.ignore_numbers && w.chars().any(|c| c.is_ascii_digit())) || !w.chars().any(char::is_alphabetic) {
        return true;
    }
    if opts.ignore_uppercase && w.chars().all(|c| !c.is_lowercase()) && n <= 6 {
        return true;
    }
    if !w.chars().any(is_latin_letter) {
        return true;
    }
    // Hyphenated compounds (`Kfz-Meister`): the whole word, else every part.
    if w.contains('-') && !known(d, w) {
        return w.split('-').all(|p| p.chars().count() <= 1 || known(d, p) || p.chars().all(|c| !c.is_lowercase()));
    }
    known(d, w)
}

fn known(d: &Dictionary, w: &str) -> bool {
    d.check(w) || crate::in_user_dictionary(&w.to_lowercase())
}

/// German spelling suggestions (best first, at most `max`), from spellbook's Hunspell search.
pub fn suggest(word: &str, max: usize) -> Vec<String> {
    let Some(d) = dictionary() else { return Vec::new() };
    let mut out = Vec::new();
    d.suggest(word, &mut out);
    out.truncate(max);
    out
}

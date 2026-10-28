//! Small synthetic UTF-8 German Hunspell dictionary for deterministic tests.
pub const AFF: &str = "SET UTF-8
TRY esnrtiaduhlgcmobfkzwpvüäößjyxq
REP 2
REP s ß
REP ss ß
SFX N Y 2
SFX N 0 n e
SFX N 0 en [^e]
SFX S Y 1
SFX S 0 s .
";
pub const DIC: &str = "12
Bremse/N
Prüfung/N
Meister/S
Straße/N
Werkstatt
und
der
die
das
prüfen
Kraftfahrzeug/S
groß
";
pub fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn install() {
    if let Err(e) = super::install_dictionary(AFF, DIC) {
        panic!("test dictionary: {e}");
    }
}

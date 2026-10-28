use super::*;

#[test]
fn test_dictionary_loads() {
    let _guard = test_dict::lock();
    test_dict::install();
    assert!(has_dictionary());
}

#[test]
fn correct_words_and_affixes() {
    let _guard = test_dict::lock();
    test_dict::install();
    for w in
        ["Bremse", "Bremsen", "Prüfung", "Prüfungen", "Meister", "Meisters", "Straße", "Straßen", "Werkstatt", "und", "Und", "groß", "KFZ", "2026"]
    {
        assert!(is_correct(w), "{w}");
    }
}

#[test]
fn misspellings_are_flagged() {
    let _guard = test_dict::lock();
    test_dict::install();
    for w in ["Bremze", "Prüffung", "Strase", "Werkstat", "prüfenn"] {
        assert!(!is_correct(w), "{w}");
    }
}

#[test]
fn suggestions_fix_typical_slips() {
    let _guard = test_dict::lock();
    test_dict::install();
    assert!(suggest("Strase", 5).iter().any(|s| s == "Straße"), "{:?}", suggest("Strase", 5));
    assert!(suggest("Prüffung", 5).iter().any(|s| s == "Prüfung"), "{:?}", suggest("Prüffung", 5));
}

#[test]
fn missing_dictionary_is_silent() {
    assert!(is_correct_with(None, "Gibtesnichtwort"));
}

#[test]
fn bytes_are_decoded_per_set_line_and_rechecked() {
    let _guard = test_dict::lock();
    // ISO 8859-1 like the LibreOffice de_DE_frami dictionary: 0xFC is ü.
    let aff = b"SET ISO8859-1\n".to_vec();
    let dic = b"1\nPr\xfcfung\n".to_vec();
    let before = crate::dictionary_generation();
    install_dictionary_bytes(&aff, &dic).expect("install");
    assert!(crate::dictionary_generation() > before, "text is checked again");
    assert!(is_correct("Prüfung"));
    assert!(!is_correct("Prufung"));
    test_dict::install();
}

#[test]
fn failed_install_keeps_the_current_dictionary() {
    let _guard = test_dict::lock();
    test_dict::install();
    assert!(install_dictionary_bytes(b"SET UTF-8\n", b"not-a-number\nPruefung\n").is_err());
    assert!(is_correct("Prüfung"));
    assert!(!is_correct("Prüffung"));
}

#[test]
fn oversized_aff_bytes_are_rejected_before_parsing() {
    let _guard = test_dict::lock();
    let oversized = vec![b' '; MAX_FILE_BYTES as usize + 1];
    let error = install_dictionary_bytes(&oversized, test_dict::DIC.as_bytes()).expect_err("oversized .aff accepted");
    assert!(error.contains(".aff: too large"), "{error}");
}

#[test]
fn oversized_dic_bytes_are_rejected_before_parsing() {
    let _guard = test_dict::lock();
    let oversized = vec![b' '; MAX_FILE_BYTES as usize + 1];
    let error = install_dictionary_bytes(test_dict::AFF.as_bytes(), &oversized).expect_err("oversized .dic accepted");
    assert!(error.contains(".dic: too large"), "{error}");
}

#[test]
fn reload_during_load_does_not_restore_stale_dictionary() {
    let _guard = test_dict::lock();
    reload_dictionary();
    let stale = Arc::new(Dictionary::new(test_dict::AFF, test_dict::DIC).expect("test dictionary"));
    let (loaded_tx, loaded_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    *TEST_LOAD_GATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((loaded_tx, resume_rx, Some(stale.clone())));
    let loading = std::thread::spawn(dictionary);
    loaded_rx.recv().expect("loader reaches the gate");
    reload_dictionary();
    resume_tx.send(()).expect("resume loader");
    loading.join().expect("loader finishes");
    let stale_cached = {
        let slot = DICT.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        slot.as_ref().and_then(|d| d.as_ref()).is_some_and(|d| Arc::ptr_eq(d, &stale))
    };
    test_dict::install();
    assert!(!stale_cached, "reload restored the stale dictionary");
}

#[test]
fn german_is_a_proofed_language_that_dispatches_here() {
    let _guard = test_dict::lock();
    test_dict::install();
    assert!(crate::ProofLang::De.is_proofed());
    assert!(crate::is_correct_in("Bremse", crate::ProofLang::De));
    assert!(!crate::is_correct_in("Bremze", crate::ProofLang::De));
    assert!(!crate::suggest_in("Strase", 5, crate::ProofLang::De).is_empty());
}

/// The real LibreOffice dictionary (`de_DE_frami`), when `WORDCRAFT_DE_DICT` names its folder:
/// compounds and the Fugen-s, as German writing needs them.
#[test]
fn real_frami_dictionary_handles_compounds() {
    let Some(dir) = std::env::var_os("WORDCRAFT_DE_DICT") else { return };
    let dir = std::path::PathBuf::from(dir);
    let d = load_files(&dir.join("de_DE_frami.aff"), &dir.join("de_DE_frami.dic")).expect("load");
    for w in [
        "Bremsflüssigkeit",
        "Bremsflüssigkeitsprüfung",
        "Meisterprüfung",
        "Kraftfahrzeugmechatroniker",
        "Straße",
        "STRASSE",
        "Werkstatt",
        "Handwerkskammer",
        "Und",
        "prüfen",
        "Kfz-Meister",
    ] {
        assert!(is_correct_with(Some(&d), w), "{w}");
    }
    for w in ["Bremsflüsigkeit", "Strase", "Meisterprüffung", "Handwerkskamer"] {
        assert!(!is_correct_with(Some(&d), w), "{w}");
    }
    let mut s = Vec::new();
    d.suggest("Meisterprüffung", &mut s);
    assert!(s.iter().any(|x| x == "Meisterprüfung"), "{s:?}");
}

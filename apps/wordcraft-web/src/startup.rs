//! Startup behavior shared by the browser shell and native regression tests.

use wordcraft_ui_egui::WordApp;

pub fn apply_nextcloud_open_result(app: &mut WordApp, confirmed_new: bool) {
    if confirmed_new {
        app.session.apply_editing_language_to_blank_document();
    }
}

pub struct DownloadBudget {
    received: u64,
    limit: u64,
}

impl DownloadBudget {
    pub fn new(limit: u64) -> Self {
        Self { received: 0, limit }
    }

    pub fn record_chunk(&mut self, chunk_len: u64) -> Result<(), String> {
        let next = self.received.checked_add(chunk_len).ok_or("download size overflow")?;
        if next > self.limit {
            return Err(format!("download exceeds {limit} bytes", limit = self.limit));
        }
        self.received = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wordcraft_engine::Session;
    use wordcraft_ui_egui::{Services, WordApp};

    fn app() -> WordApp {
        WordApp::new(Session::new(wordcraft_engine::blank_document(&Default::default())), Services::default())
    }

    #[test]
    fn confirmed_nextcloud_404_applies_the_editing_language_to_the_blank_document() {
        let mut a = app();
        a.session.path = Some(std::path::PathBuf::from("Ausarbeitungen/Neu.docx"));
        a.session.set_editing_language(Some("de-DE"));
        assert_eq!(a.session.doc.styles.default_chr.lang.as_deref(), Some("en-US"));

        apply_nextcloud_open_result(&mut a, true);

        assert_eq!(a.session.doc.styles.default_chr.lang.as_deref(), Some("de-DE"));
    }

    #[test]
    fn downloaded_nextcloud_document_keeps_its_own_language() {
        let mut a = app();
        a.session.doc.styles.default_chr.lang = Some("fr-FR".into());
        a.session.path = Some(std::path::PathBuf::from("Ausarbeitungen/Vorhanden.docx"));
        a.session.set_editing_language(Some("de-DE"));

        apply_nextcloud_open_result(&mut a, false);

        assert_eq!(a.session.doc.styles.default_chr.lang.as_deref(), Some("fr-FR"));
    }

    #[test]
    fn dictionary_budget_rejects_an_oversized_chunk_before_accepting_it() {
        let mut budget = DownloadBudget::new(4);
        budget.record_chunk(3).unwrap();
        assert!(budget.record_chunk(2).is_err());
        assert_eq!(budget.received, 3);
        let mut max = DownloadBudget::new(u64::MAX);
        assert!(max.record_chunk(u64::MAX).is_ok());
        assert!(max.record_chunk(1).is_err());
    }
}

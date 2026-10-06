use std::{collections::HashMap, sync::Mutex};

#[derive(Default)]
pub struct PendingFiles(Mutex<HashMap<String, String>>);

impl PendingFiles {
    pub fn set(&self, label: &str, path: String) {
        self.0.lock().unwrap().insert(label.to_owned(), path);
    }

    pub fn take(&self, label: &str) -> Option<String> {
        self.0.lock().unwrap().remove(label)
    }

    pub fn has(&self, label: &str) -> bool {
        self.0.lock().unwrap().contains_key(label)
    }
}

#[derive(Default)]
pub struct Documents(Mutex<HashMap<String, String>>);

impl Documents {
    pub fn open(&self, label: &str, path: &str) {
        self.0.lock().unwrap().insert(label.to_owned(), path.to_owned());
    }

    pub fn close(&self, label: &str) {
        self.0.lock().unwrap().remove(label);
    }

    pub fn has(&self, label: &str) -> bool {
        self.0.lock().unwrap().contains_key(label)
    }

    pub fn label_of(&self, path: &str) -> Option<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find(|(_, p)| p.as_str() == path)
            .map(|(label, _)| label.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_is_per_window_and_taken_once() {
        let pending = PendingFiles::default();
        pending.set("main", "a.md".into());
        pending.set("w2", "b.md".into());
        assert!(pending.has("main"));
        assert_eq!(pending.take("main").as_deref(), Some("a.md"));
        assert_eq!(pending.take("main"), None);
        assert_eq!(pending.take("w2").as_deref(), Some("b.md"));
    }

    #[test]
    fn documents_track_which_window_shows_which_file() {
        let docs = Documents::default();
        assert!(!docs.has("main"));
        docs.open("main", "a.md");
        docs.open("w2", "b.md");
        assert_eq!(docs.label_of("b.md").as_deref(), Some("w2"));
        assert_eq!(docs.label_of("c.md"), None);
        docs.open("w2", "c.md");
        assert_eq!(docs.label_of("b.md"), None);
        docs.close("w2");
        assert!(!docs.has("w2"));
    }
}

use std::{
    collections::{BTreeSet, HashMap},
    sync::Mutex,
};

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
pub struct DirtyWindows(Mutex<BTreeSet<String>>);

impl DirtyWindows {
    pub fn set(&self, label: &str, dirty: bool) {
        let mut labels = self.0.lock().unwrap();
        if dirty {
            labels.insert(label.to_owned());
        } else {
            labels.remove(label);
        }
    }

    pub fn labels(&self) -> Vec<String> {
        self.0.lock().unwrap().iter().cloned().collect()
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

    pub fn owns(&self, label: &str, path: &str) -> bool {
        let registered = self.0.lock().unwrap().get(label).cloned();
        let Some(registered) = registered else { return false };
        if registered == path {
            return true;
        }
        match (std::fs::canonicalize(&registered), std::fs::canonicalize(path)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
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
    fn dirty_windows_are_tracked_per_label() {
        let dirty = DirtyWindows::default();
        assert!(dirty.labels().is_empty());
        dirty.set("a", true);
        dirty.set("b", true);
        dirty.set("a", true);
        assert_eq!(dirty.labels(), vec!["a", "b"]);
        dirty.set("a", false);
        assert_eq!(dirty.labels(), vec!["b"]);
    }

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
    fn a_window_owns_only_the_file_it_has_open() {
        let dir = std::env::temp_dir().join(format!("washi-owns-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.md"), dir.join("b.md"));
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let docs = Documents::default();
        docs.open("main", a.to_str().unwrap());
        assert!(docs.owns("main", a.to_str().unwrap()));
        assert!(!docs.owns("main", b.to_str().unwrap()), "別のファイルには書かせない");
        assert!(!docs.owns("other", a.to_str().unwrap()), "開いていないウィンドウには書かせない");
        assert!(!docs.owns("main", "/etc/hosts"));
        let dotted = format!("{}/./a.md", dir.display());
        assert!(docs.owns("main", &dotted));
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

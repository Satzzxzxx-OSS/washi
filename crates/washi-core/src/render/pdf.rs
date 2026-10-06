use std::{fs, path::Path};

use super::{Output, Renderer};

pub struct PdfRenderer;

impl Renderer for PdfRenderer {
    fn name(&self) -> &'static str {
        "pdf"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn render(&self, path: &Path) -> Result<Output, String> {
        fs::read(path)
            .map(Output::Pdf)
            .map_err(|e| format!("{}: {e}", path.display()))
    }
}

use std::{fs, path::Path};

use super::{markdown, Output, Renderer};

pub struct MermaidRenderer;

impl Renderer for MermaidRenderer {
    fn name(&self) -> &'static str {
        "mermaid"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["mmd", "mermaid"]
    }

    fn render(&self, path: &Path) -> Result<Output, String> {
        let source = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        markdown::render_source(&fence(&source), path.parent())
    }

    fn render_text(&self, source: &str) -> Result<Output, String> {
        markdown::render_source(&fence(source), None)
    }
}

fn fence(source: &str) -> String {
    format!("```mermaid\n{}\n```\n", source.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_the_source_in_a_mermaid_block() {
        let dir = std::env::temp_dir().join(format!("washi-mmd-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.mmd");
        fs::write(&file, "graph LR; A-->B\n").unwrap();
        match MermaidRenderer.render(&file).unwrap() {
            Output::Html(html) => {
                assert!(html.contains("language-mermaid"), "{html}");
                assert!(html.contains("A--&gt;B"), "{html}");
            }
            Output::Pdf(_) => panic!("HTML を期待"),
        }
    }
}

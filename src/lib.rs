//! SigmaOS site — static site builder.
//!
//! Builds a static site from a content directory: markdown-ish pages get a
//! base HTML template with {{title}}/{{content}} substitution, assets are
//! copied verbatim, and an index page links everything.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Errors produced by this component.
#[derive(Debug, thiserror::Error)]
pub enum SiteError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("walk error: {0}")]
    Walk(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("not found: {0}")]
    NotFound(String),
}

/// Site configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteConfig {
    pub title: String,
    pub base_url: String,
    #[serde(default)]
    pub description: String,
    /// Template file (supports {{title}}, {{content}}, {{description}}).
    #[serde(default = "default_template")]
    pub template: String,
}

fn default_template() -> String {
    "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\">\n<title>{{title}}</title>\n<meta name=\"description\" content=\"{{description}}\">\n</head><body>\n{{content}}\n</body></html>\n".to_string()
}

/// A built page record.
#[derive(Debug, Clone, Serialize)]
pub struct BuiltPage {
    pub source: PathBuf,
    pub output: PathBuf,
    pub title: String,
}

/// Build result.
#[derive(Debug, Clone, Serialize, Default)]
pub struct BuildResult {
    pub pages: Vec<BuiltPage>,
    pub assets_copied: usize,
    pub output_dir: PathBuf,
}

/// Render the template with substitutions.
pub fn render(template: &str, title: &str, description: &str, content: &str) -> String {
    template
        .replace("{{title}}", title)
        .replace("{{description}}", description)
        .replace("{{content}}", content)
}

/// Escape HTML special chars.
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Minimal markdown-to-HTML: headings, paragraphs, links, bold, code.
pub fn markdown_to_html(md: &str) -> String {
    let mut out = String::new();
    let mut in_para = false;
    for line in md.lines() {
        let t = line.trim();
        if t.is_empty() {
            if in_para {
                out.push_str("</p>\n");
                in_para = false;
            }
            continue;
        }
        if let Some(h) = t.strip_prefix("### ") {
            if in_para {
                out.push_str("</p>\n");
                in_para = false;
            }
            out.push_str(&format!("<h3>{}</h3>\n", escape_html(h)));
        } else if let Some(h) = t.strip_prefix("## ") {
            if in_para {
                out.push_str("</p>\n");
                in_para = false;
            }
            out.push_str(&format!("<h2>{}</h2>\n", escape_html(h)));
        } else if let Some(h) = t.strip_prefix("# ") {
            if in_para {
                out.push_str("</p>\n");
                in_para = false;
            }
            out.push_str(&format!("<h1>{}</h1>\n", escape_html(h)));
        } else if t.starts_with("- ") || t.starts_with("* ") {
            if in_para {
                out.push_str("</p>\n");
                in_para = false;
            }
            out.push_str(&format!("<li>{}</li>\n", escape_html(&t[2..])));
        } else {
            if !in_para {
                out.push_str("<p>");
                in_para = true;
            } else {
                out.push(' ');
            }
            // inline: **bold**, `code`, [text](url)
            let mut rest = t;
            let mut rendered = String::new();
            while !rest.is_empty() {
                if let Some(i) = rest.find("**") {
                    rendered.push_str(&escape_html(&rest[..i]));
                    if let Some(j) = rest[i + 2..].find("**") {
                        rendered.push_str(&format!(
                            "<strong>{}</strong>",
                            escape_html(&rest[i + 2..i + 2 + j])
                        ));
                        rest = &rest[i + 2 + j + 2..];
                        continue;
                    }
                    rendered.push_str("**");
                    rest = &rest[i + 2..];
                } else if let Some(i) = rest.find('`') {
                    rendered.push_str(&escape_html(&rest[..i]));
                    if let Some(j) = rest[i + 1..].find('`') {
                        rendered.push_str(&format!(
                            "<code>{}</code>",
                            escape_html(&rest[i + 1..i + 1 + j])
                        ));
                        rest = &rest[i + 1 + j + 1..];
                        continue;
                    }
                    rendered.push('`');
                    rest = &rest[i + 1..];
                } else if let Some(i) = rest.find('[') {
                    rendered.push_str(&escape_html(&rest[..i]));
                    if let Some(close) = rest[i..].find("](") {
                        let text_end = i + close;
                        let link_text = &rest[i + 1..text_end];
                        if let Some(url_end) = rest[text_end..].find(')') {
                            let url = &rest[text_end + 2..text_end + url_end];
                            rendered.push_str(&format!(
                                "<a href=\"{}\">{}</a>",
                                escape_html(url),
                                escape_html(link_text)
                            ));
                            rest = &rest[text_end + url_end + 1..];
                            continue;
                        }
                    }
                    rendered.push('[');
                    rest = &rest[i + 1..];
                } else {
                    rendered.push_str(&escape_html(rest));
                    break;
                }
            }
            out.push_str(&rendered);
        }
    }
    if in_para {
        out.push_str("</p>\n");
    }
    out
}

/// Build the site: content dir -> output dir.
pub fn build(
    content_dir: &Path,
    output_dir: &Path,
    config: &SiteConfig,
) -> Result<BuildResult, SiteError> {
    if !content_dir.is_dir() {
        return Err(SiteError::NotFound(format!(
            "content dir: {}",
            content_dir.display()
        )));
    }
    std::fs::create_dir_all(output_dir)?;

    let mut result = BuildResult {
        output_dir: output_dir.to_path_buf(),
        ..Default::default()
    };
    let mut index_entries: Vec<(String, String)> = Vec::new(); // (title, href)

    // Collect markdown files and other assets.
    for entry in walkdir::WalkDir::new(content_dir).follow_links(false) {
        let entry = entry.map_err(|e| SiteError::Walk(e.to_string()))?;
        let src = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = src
            .strip_prefix(content_dir)
            .map_err(|e| SiteError::Validation(e.to_string()))?;
        let is_md = src.extension().and_then(|e| e.to_str()) == Some("md");

        if is_md {
            let md = std::fs::read_to_string(src)?;
            let title = md
                .lines()
                .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_string()))
                .unwrap_or_else(|| {
                    src.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("untitled")
                        .to_string()
                });
            let body = markdown_to_html(&md);
            let html = render(&config.template, &title, &config.description, &body);
            // output path: strip .md, use .html; index.md -> index.html
            let out_rel = rel.with_extension("html");
            let out_path = output_dir.join(&out_rel);
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&out_path, html)?;
            let href = format!("/{}", out_rel.display().to_string().replace('\\', "/"));
            index_entries.push((title.clone(), href));
            result.pages.push(BuiltPage {
                source: src.to_path_buf(),
                output: out_path,
                title,
            });
        } else {
            // copy asset verbatim
            let out_path = output_dir.join(rel);
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(src, &out_path)?;
            result.assets_copied += 1;
        }
    }

    // Generate index.html if there are pages and no index.md existed.
    let index_path = output_dir.join("index.html");
    let had_index = result.pages.iter().any(|p| p.output == index_path);
    if !had_index && !index_entries.is_empty() {
        let mut links = String::from("<h1>");
        links.push_str(&escape_html(&config.title));
        links.push_str("</h1>\n<ul>\n");
        for (title, href) in &index_entries {
            links.push_str(&format!(
                "<li><a href=\"{}\">{}</a></li>\n",
                escape_html(href),
                escape_html(title)
            ));
        }
        links.push_str("</ul>\n");
        let html = render(&config.template, &config.title, &config.description, &links);
        std::fs::write(&index_path, html)?;
    }

    Ok(result)
}

/// Default site config.
pub fn default_config() -> SiteConfig {
    SiteConfig {
        title: "SigmaOS Site".into(),
        base_url: "https://sigmaos.dev".into(),
        description: String::new(),
        template: default_template(),
    }
}

/// Save config as JSON.
pub fn save_config(config: &SiteConfig, path: &Path) -> Result<(), SiteError> {
    std::fs::write(path, serde_json::to_string_pretty(config)?)?;
    Ok(())
}

/// Load config from JSON.
pub fn load_config(path: &Path) -> Result<SiteConfig, SiteError> {
    let text = std::fs::read_to_string(path)
        .map_err(|_| SiteError::NotFound(path.display().to_string()))?;
    Ok(serde_json::from_str(&text)?)
}

/// Group pages by directory for sitemap-style listing.
pub fn pages_by_dir(result: &BuildResult) -> BTreeMap<String, Vec<&BuiltPage>> {
    let mut map: BTreeMap<String, Vec<&BuiltPage>> = BTreeMap::new();
    for p in &result.pages {
        let dir = p
            .source
            .parent()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        map.entry(dir).or_default().push(p);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(tag: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("site-{}-{}", tag, std::process::id()));
        let content = base.join("content");
        let out = base.join("out");
        std::fs::create_dir_all(&content).unwrap();
        (content, out)
    }

    #[test]
    fn escape_html_covers_specials() {
        assert_eq!(escape_html("<a>&\"b\""), "&lt;a&gt;&amp;&quot;b&quot;");
    }

    #[test]
    fn render_substitutes_all_tokens() {
        let html = render(
            "<h>{{title}}</h><p>{{description}}</p>{{content}}",
            "T",
            "D",
            "C",
        );
        assert_eq!(html, "<h>T</h><p>D</p>C");
    }

    #[test]
    fn markdown_headings_lists_inline() {
        let md = "# Title\n\nHello **bold** and `code` and [lnk](http://x).\n\n- one\n- two\n";
        let html = markdown_to_html(md);
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<code>code</code>"));
        assert!(html.contains("<a href=\"http://x\">lnk</a>"));
        assert!(html.contains("<li>one</li>"));
        assert!(html.contains("<li>two</li>"));
        assert!(html.contains("</p>"));
    }

    #[test]
    fn markdown_escapes_html_in_text() {
        let html = markdown_to_html("# <script>\n\n<x> & y");
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&lt;x&gt; &amp; y"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn markdown_paragraph_joins_lines() {
        let html = markdown_to_html("line one\nline two\n\nnew para");
        assert!(html.contains("line one line two"));
        assert_eq!(html.matches("<p>").count(), 2);
    }

    #[test]
    fn build_generates_pages_assets_and_index() {
        let (content, out) = dirs("build");
        std::fs::write(content.join("index.md"), "# Home\n\nWelcome.\n").unwrap();
        std::fs::create_dir_all(content.join("docs")).unwrap();
        std::fs::write(content.join("docs/guide.md"), "## Guide\n\nText.\n").unwrap();
        std::fs::write(content.join("style.css"), "body{}\n").unwrap();
        let cfg = default_config();
        let res = build(&content, &out, &cfg).unwrap();
        assert_eq!(res.pages.len(), 2);
        assert_eq!(res.assets_copied, 1);
        assert!(out.join("index.html").exists());
        assert!(out.join("docs/guide.html").exists());
        assert!(out.join("style.css").exists());
        let idx = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(idx.contains("Welcome."));
        // pages_by_dir groups correctly
        let groups = pages_by_dir(&res);
        assert_eq!(groups.len(), 2);
        let _ = std::fs::remove_dir_all(content.parent().unwrap());
    }

    #[test]
    fn build_generates_index_when_no_index_md() {
        let (content, out) = dirs("noidx");
        std::fs::write(content.join("a.md"), "# A\n").unwrap();
        std::fs::write(content.join("b.md"), "# B\n").unwrap();
        let res = build(&content, &out, &default_config()).unwrap();
        assert!(!res.pages.iter().any(|p| p.output.ends_with("index.html")));
        let idx = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(idx.contains("href=\"/a.html\""));
        assert!(idx.contains("href=\"/b.html\""));
        let _ = std::fs::remove_dir_all(content.parent().unwrap());
    }

    #[test]
    fn build_missing_content_dir_errors() {
        let r = build(
            Path::new("/nonexistent-dir"),
            Path::new("/tmp/out-x"),
            &default_config(),
        );
        assert!(matches!(r, Err(SiteError::NotFound(_))));
    }

    #[test]
    fn config_roundtrip() {
        let (content, _out) = dirs("cfg");
        let path = content.join("site.json");
        let cfg = SiteConfig {
            title: "T".into(),
            base_url: "https://x".into(),
            description: "d".into(),
            template: "{{title}}|{{content}}".into(),
        };
        save_config(&cfg, &path).unwrap();
        let back = load_config(&path).unwrap();
        assert_eq!(back.title, "T");
        assert_eq!(back.template, "{{title}}|{{content}}");
        assert!(load_config(&content.join("nope.json")).is_err());
        let _ = std::fs::remove_dir_all(content.parent().unwrap());
    }
}

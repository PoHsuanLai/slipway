//! A builder for `release.toml` text, so each test changes one thing.

use slipway_core::Sha256Hex;

/// One file of the fake bundle.
#[derive(Clone)]
pub struct F {
    pub path: String,
    pub content: String,
    pub mode: String,
}

pub fn f(path: &str, content: &str) -> F {
    F {
        path: path.to_owned(),
        content: content.to_owned(),
        mode: "0o644".to_owned(),
    }
}

/// The parts of a release description; `render` turns them into TOML text.
#[derive(Clone)]
pub struct Doc {
    pub format: String,
    pub version: String,
    pub date: String,
    pub notes: String,
    pub bundle_name: String,
    pub bundle_sha: String,
    pub bundle_size: String,
    pub files: Vec<F>,
    /// (link, target)
    pub exports: Vec<(String, String)>,
}

impl Doc {
    pub fn good() -> Self {
        Self {
            format: "1".into(),
            version: "0.2.0".into(),
            date: "\"2026-10-20T08:00:00Z\"".into(),
            notes: "Faster start.".into(),
            bundle_name: "bundle.tar.zst".into(),
            bundle_sha: Sha256Hex::of(b"bundle").to_string(),
            bundle_size: "123".into(),
            files: vec![
                f("bin/casement", "casement"),
                f("share/wayland-sessions/quire.desktop", "[Desktop Entry]"),
            ],
            exports: vec![(
                "/usr/share/wayland-sessions/quire.desktop".into(),
                "share/wayland-sessions/quire.desktop".into(),
            )],
        }
    }

    pub fn render(&self) -> String {
        let mut text = format!(
            "format = {}\nversion = \"{}\"\ndate = {}\nnotes = \"{}\"\nbundle = {{ name = \"{}\", sha256 = \"{}\", size = {} }}\n[components]\ncasement = \"e616a79\"\n",
            self.format,
            self.version,
            self.date,
            self.notes,
            self.bundle_name,
            self.bundle_sha,
            self.bundle_size
        );
        for file in &self.files {
            text += &format!(
                "[[file]]\npath = \"{}\"\nsha256 = \"{}\"\nmode = {}\n",
                file.path,
                Sha256Hex::of(file.content.as_bytes()),
                file.mode
            );
        }
        for (link, target) in &self.exports {
            text += &format!("[[export]]\nlink = \"{link}\"\ntarget = \"{target}\"\n");
        }
        text
    }
}

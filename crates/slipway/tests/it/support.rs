//! Building signed-looking bundles and a sandbox to install them into.

use semver::Version;
use slipway::Layout;
use slipway_core::{Authenticator, Sha256Hex, SignError};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Accepts any signature: the signature check itself is tested in slipway-core with a real pair.
pub struct Accept;

impl Authenticator for Accept {
    fn authenticate(&self, _: &[u8], _: &str) -> Result<(), SignError> {
        Ok(())
    }
}

/// Refuses every signature.
pub struct Reject;

impl Authenticator for Reject {
    fn authenticate(&self, _: &[u8], _: &str) -> Result<(), SignError> {
        Err(SignError::NotSigned)
    }
}

pub fn v(text: &str) -> Version {
    Version::parse(text).expect("version")
}

/// A release and its bundle, as CI would publish them.
pub struct Built {
    pub version: Version,
    pub release_toml: String,
    pub bundle: Vec<u8>,
}

/// What goes in a built release.
pub struct Recipe<'a> {
    pub version: &'a str,
    /// (path, content) the release lists.
    pub listed: Vec<(&'a str, &'a str)>,
    /// (path, content) actually packed, when different from `listed`.
    pub packed: Option<Vec<(&'a str, &'a str)>>,
    /// (link, target) exports.
    pub exports: Vec<(&'a str, &'a str)>,
}

impl<'a> Recipe<'a> {
    pub fn new(version: &'a str) -> Self {
        Self {
            version,
            listed: vec![
                ("bin/casement", "casement 1"),
                ("share/wayland-sessions/quire.desktop", "[Desktop Entry]"),
            ],
            packed: None,
            exports: vec![(
                "/usr/share/wayland-sessions/quire.desktop",
                "share/wayland-sessions/quire.desktop",
            )],
        }
    }
}

fn mode_of(path: &str) -> &'static str {
    if path.starts_with("bin/") {
        "0o755"
    } else {
        "0o644"
    }
}

fn pack(items: &[(&str, &str)]) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    let mut dirs: Vec<String> = Vec::new();
    for (path, _) in items {
        if let Some((dir, _)) = path.rsplit_once('/')
            && !dirs.iter().any(|d| d == dir)
        {
            dirs.push(dir.to_owned());
        }
    }
    for dir in dirs {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        tar.append_data(&mut header, format!("{dir}/"), std::io::empty())
            .expect("dir");
    }
    for (path, content) in items {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(content.len() as u64);
        header.set_mode(0o600);
        tar.append_data(&mut header, path, content.as_bytes())
            .expect("file");
    }
    zstd::encode_all(&tar.into_inner().expect("tar")[..], 3).expect("zstd")
}

/// A tar.zst whose only entry is `path` of the given type (for hostile entries).
pub fn pack_special(honest: &[(&str, &str)], path: &str, kind: tar::EntryType) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for (p, content) in honest {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        tar.append_data(&mut header, p, content.as_bytes())
            .expect("file");
    }
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(kind);
    header.set_size(0);
    header.set_mode(0o777);
    header
        .set_link_name_literal(b"/etc/passwd")
        .expect("link name");
    tar.append_data(&mut header, path, std::io::empty())
        .expect("special");
    zstd::encode_all(&tar.into_inner().expect("tar")[..], 3).expect("zstd")
}

/// The release.toml for a bundle.
pub fn describe(
    version: &str,
    listed: &[(&str, &str)],
    exports: &[(&str, &str)],
    bundle: &[u8],
) -> String {
    let mut text = format!(
        "format = 1\nversion = \"{version}\"\ndate = \"2026-10-20T08:00:00Z\"\nnotes = \"Notes for {version}\"\nbundle = {{ name = \"bundle.tar.zst\", sha256 = \"{}\", size = {} }}\n[components]\ncasement = \"e616a79\"\n",
        Sha256Hex::of(bundle),
        bundle.len()
    );
    for (path, content) in listed {
        text += &format!(
            "[[file]]\npath = \"{path}\"\nsha256 = \"{}\"\nmode = {}\n",
            Sha256Hex::of(content.as_bytes()),
            mode_of(path)
        );
    }
    for (link, target) in exports {
        text += &format!("[[export]]\nlink = \"{link}\"\ntarget = \"{target}\"\n");
    }
    text
}

pub fn build(recipe: &Recipe<'_>) -> Built {
    let bundle = pack(recipe.packed.as_deref().unwrap_or(&recipe.listed));
    Built {
        version: v(recipe.version),
        release_toml: describe(recipe.version, &recipe.listed, &recipe.exports, &bundle),
        bundle,
    }
}

/// Builds a release around exactly these bundle bytes.
pub fn build_around(recipe: &Recipe<'_>, bundle: Vec<u8>) -> Built {
    Built {
        version: v(recipe.version),
        release_toml: describe(recipe.version, &recipe.listed, &recipe.exports, &bundle),
        bundle,
    }
}

/// An install root and a stand-in for `/`, in a temporary directory.
pub struct Sandbox {
    pub tmp: TempDir,
    pub layout: Layout,
}

impl Sandbox {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let layout = Layout {
            root: tmp.path().join("opt/quire"),
            export_base: tmp.path().join("fs"),
        };
        Self { tmp, layout }
    }

    /// Writes a built release into a fresh staging directory and returns it.
    pub fn stage(&self, built: &Built) -> PathBuf {
        let dir = self.tmp.path().join(format!(
            "stage-{}-{}",
            built.version,
            Sha256Hex::of(&built.bundle)
        ));
        std::fs::create_dir_all(&dir).expect("stage dir");
        std::fs::write(dir.join("release.toml"), &built.release_toml).expect("release.toml");
        std::fs::write(dir.join("release.toml.minisig"), "signature").expect("signature");
        std::fs::write(dir.join("bundle.tar.zst"), &built.bundle).expect("bundle");
        dir
    }

    /// Stages and installs a release with the accepting authenticator.
    pub fn install(
        &self,
        built: &Built,
    ) -> Result<slipway::apply::Applied, slipway::apply::ApplyError> {
        slipway::apply::install(&self.layout, &self.stage(built), &Accept)
    }

    /// Where an export link lives in the stand-in for `/`.
    pub fn link(&self, link: &str) -> PathBuf {
        self.layout.export_path(link)
    }

    pub fn current(&self) -> Option<Version> {
        self.layout.installed().expect("installed").current
    }

    pub fn previous(&self) -> Option<Version> {
        self.layout.installed().expect("installed").previous
    }

    pub fn releases(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.layout.releases_dir())
            .map(|d| {
                d.map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read")
}

//! Horizon's mod registry from inside the game (Options > Mods > Browse mods,
//! docs/design/mod-hotload.md): the same `index.json` the launcher reads, the
//! projects this game can install, and installing one exactly as the launcher
//! does (`launcher/src/browser/horizon.rs`, `install_version`): downloaded into
//! `mod-browser/downloads/`, checked against the index (size, SHA-256, the
//! author's signature, the file's own manifest), moved into `horizon-mods/`
//! and recorded in `mod-browser.json`, so the launcher lists, updates and
//! removes it as one of its own.
//!
//! The launcher is a separate workspace, so this is the small part of it the
//! game needs. Everything here blocks: the game runs it on a worker
//! (`game/mod_browser.rs`).
use crate::mod_browser::Listing;
use horizon_mods::index::{self, Index, Kind, Project, Trust, Version};
use std::fs;
#[cfg(not(target_os = "emscripten"))]
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where the registry publishes its index (the launcher's `INDEX_URL`).
pub const INDEX_URL: &str = "https://horizon-rust.github.io/horizon-mods/index.json";
/// The Minecraft version this game is: the registry's `game`.
pub const GAME: &str = horizon_core::version::VERSION_ID;
/// The registry's pages on the website, `<SITE>/<id>`.
pub const SITE: &str = "https://playhorizon.dev/mods";
/// The last index fetched, beside `options.txt`: the launcher keeps its own copy there too.
pub const CACHE: &str = "mod-browser/horizon-index.json";
/// What the launcher's browser installed (`installed::MANIFEST`).
pub const MANIFEST: &str = "mod-browser.json";
/// Where a download waits for its checks.
const DOWNLOADS: &str = "mod-browser/downloads";
/// Larger than any index should be.
const MAX_INDEX: u64 = 16 << 20;
/// The game asks the registry by this name.
const AGENT: &str = concat!(
    "horizon-rust/horizon/",
    env!("CARGO_PKG_VERSION"),
    " (in-game mod browser)"
);

/// Where the index comes from: `HORIZON_MOD_INDEX` (a file or an `https://`
/// address, for a test registry), else [`INDEX_URL`]; as the launcher reads it.
pub fn location() -> String {
    std::env::var("HORIZON_MOD_INDEX")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| INDEX_URL.to_owned())
}

/// A local test registry: its files may be `file://` paths.
fn is_local(at: &str) -> bool {
    !at.starts_with("https://")
}

/// The index from `at`, kept in `home`'s [`CACHE`]; the kept copy when the
/// registry can't be reached.
pub fn load_index(home: &Path, at: &str) -> Result<Index, String> {
    let local = is_local(at);
    let parse = |text: &str| match local {
        true => Index::parse_local(text),
        false => Index::parse(text),
    };
    let fetched = fetch_text(at).and_then(|text| parse(&text).map(|index| (index, text)));
    let cache = home.join(CACHE);
    match fetched {
        Ok((index, text)) => {
            if let Some(dir) = cache.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(&cache, text);
            Ok(index)
        }
        Err(e) => {
            let kept = fs::read_to_string(&cache).map_err(|_| e.clone())?;
            parse(&kept).map_err(|_| e)
        }
    }
}

fn fetch_text(at: &str) -> Result<String, String> {
    if !at.starts_with("https://") {
        return fs::read_to_string(at).map_err(|e| format!("{at}: {e}"));
    }
    #[cfg(target_os = "emscripten")]
    {
        let bytes = horizon_system::web_http::get(at, MAX_INDEX)?;
        return String::from_utf8(bytes).map_err(|e| e.to_string());
    }
    #[cfg(not(target_os = "emscripten"))]
    {
        let response = ureq::get(at)
            .set("User-Agent", AGENT)
            .call()
            .map_err(|e| match e {
                ureq::Error::Status(code, _) => format!("the mod registry answered {code}"),
                ureq::Error::Transport(t) => format!("the mod registry can't be reached: {t}"),
            })?;
        let mut text = String::new();
        response
            .into_reader()
            .take(MAX_INDEX)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        Ok(text)
    }
}

/// Whether version `v` is a `.hmod` with code: the game doesn't run those any
/// more, and the launcher neither lists nor installs them (`needs_port`).
fn needs_port(v: &Version) -> bool {
    v.file.name.ends_with(".hmod")
        && v.capabilities
            .iter()
            .any(|c| *c != horizon_mods::Capability::Registries)
}

/// The version of content mod `p` this game installs: the newest for this
/// Minecraft version that wasn't taken down and has no code.
pub fn installable<'a>(index: &Index, p: &'a Project) -> Option<&'a Version> {
    let level = horizon_mods::api_level::LEVEL;
    match p.kind {
        Kind::Mod => index.installable(p, GAME, level).filter(|v| !needs_port(v)),
        // A resource pack is a file for the game's own pack list: it needs no code.
        Kind::ResourcePack => index.installable(p, GAME, level),
        // Native mods are compiled into the desktop game, shader packs need its renderer and
        // modpacks hold native mods: a browser can't run them.
        _ => None,
    }
}

/// What kind of project `p` is, as the browser's list names it.
fn kind_name(p: &Project) -> String {
    match p.kind {
        Kind::Mod if p.versions.iter().any(needs_port) => "code-mod".to_owned(),
        _ => p.kind.name().to_owned(),
    }
}

/// The registry's projects the browser offers, most downloaded first: content
/// mods this game installs, with the version installed in `home` when there is
/// one.
pub fn listings(index: &Index, home: &Path) -> Vec<Listing> {
    let installed = Installed::load(home);
    let mut out: Vec<Listing> = index
        .projects
        .iter()
        .filter_map(|p| {
            // Everything the registry has for this game is listed; what a browser can't run
            // says so (`Listing::supported`).
            let runnable = installable(index, p);
            let v = runnable.or_else(|| {
                index.installable(p, GAME, horizon_mods::api_level::LEVEL)
            })?;
            Some(Listing {
                kind: kind_name(p),
                supported: runnable.is_some(),
                id: p.id.clone(),
                name: p.name.clone(),
                summary: p.summary.clone(),
                description: p.description.clone(),
                authors: p.authors.clone(),
                license: p.license.clone(),
                categories: p.categories.clone(),
                tags: p.tags.clone(),
                version: v.version.clone(),
                published: v.published.clone(),
                changelog: v.changelog.clone(),
                downloads: p.downloads,
                page: format!("{SITE}/{}", p.id),
                icon: p.icon.clone().unwrap_or_default(),
                installed: installed.version_of(home, &p.id),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.supported
            .cmp(&a.supported)
            .then(b.downloads.cmp(&a.downloads))
            .then(a.name.cmp(&b.name))
    });
    out
}

/// What [`install`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Done {
    pub id: String,
    pub version: String,
    /// Checked against the author's key, or unsigned (the SHA-256 still pins
    /// the reviewed file).
    pub verified: bool,
    /// A resource pack, put in `resourcepacks/` and not turned on by installing.
    pub pack: bool,
}

/// Installs a resource pack of the registry into `resourcepacks/<id>.zip`: the download is checked
/// against the index's size and SHA-256, and recorded in `mod-browser.json` like a mod.
fn install_pack(
    home: &Path,
    p: &Project,
    v: &Version,
    get: &dyn Fn(&str, u64) -> Result<Vec<u8>, String>,
) -> Result<Done, String> {
    use sha2::{Digest, Sha256};
    let relative = format!("resourcepacks/{}.zip", p.id);
    let target = home.join(&relative);
    let mut list = Installed::load(home);
    let done = |verified| Done {
        id: p.id.clone(),
        version: v.version.clone(),
        verified,
        pack: true,
    };
    if let Some(i) = list.position(&p.id)
        && list.entries[i]["version"].as_str() == Some(v.version.as_str())
        && target.exists()
    {
        return Ok(done(true));
    }
    let bytes = get(&v.file.url, v.file.size)?;
    if bytes.len() as u64 != v.file.size {
        return Err(format!("{} isn't the size the index says", v.file.name));
    }
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !hash.eq_ignore_ascii_case(&v.file.sha256) {
        return Err(format!("{} doesn't match the reviewed file", v.file.name));
    }
    fs::create_dir_all(home.join("resourcepacks")).map_err(|e| e.to_string())?;
    let part = home.join("resourcepacks").join(format!(".{}.zip.part", p.id));
    fs::write(&part, &bytes).map_err(|e| format!("{}: {e}", part.display()))?;
    fs::rename(&part, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    list.record(serde_json::json!({
        "source": "horizon",
        "project": p.id,
        "slug": p.id,
        "title": p.name,
        "kind": "resourcepack",
        "version": v.version,
        "number": v.version,
        "file": relative,
        "checksum": format!("SHA-256:{}", v.file.sha256),
        "page": format!("{SITE}/{}", p.id),
        "dependency": false,
        "verdict": "Verified",
    }));
    list.save(home)?;
    Ok(done(true))
}

/// Installs the newest installable version of the registry's content mod `id`
/// into the game folder `home`, as the launcher does: `horizon-mods/<id>.hmod`,
/// recorded in `mod-browser.json`. Turning it on is the caller's
/// (`horizon-mods.txt` belongs to the game's settings). `get` downloads a file
/// of at most `max` bytes (the network, or a test's files).
pub fn install(
    index: &Index,
    home: &Path,
    id: &str,
    get: &dyn Fn(&str, u64) -> Result<Vec<u8>, String>,
) -> Result<Done, String> {
    let p = index
        .project(id)
        .ok_or_else(|| format!("{id} isn't in Horizon's registry"))?;
    let v = installable(index, p)
        .ok_or_else(|| format!("{} has no version this game can install", p.name))?;
    if p.kind == Kind::ResourcePack {
        return install_pack(home, p, v, get);
    }
    let relative = format!("{}/{id}.hmod", crate::mod_list::DIR);
    let target = home.join(&relative);
    let mut list = Installed::load(home);
    let ours = list.position(id);
    let old_file = ours.and_then(|i| list.entries[i]["file"].as_str().map(str::to_owned));
    if let Some(i) = ours
        && list.entries[i]["version"].as_str() == Some(v.version.as_str())
        && target.exists()
    {
        return Ok(Done {
            id: id.to_owned(),
            version: v.version.clone(),
            verified: list.entries[i]["verdict"].as_str() == Some("Verified"),
            pack: false,
        });
    }
    // Never over a mod the player put there themselves.
    let theirs = target.exists() && old_file.as_deref() != Some(relative.as_str());
    if theirs || home.join(crate::mod_list::DIR).join(id).is_dir() {
        return Err(format!(
            "a mod with the id {id} is already in {}: move it away first",
            crate::mod_list::DIR
        ));
    }
    if let Some(other) = list.taken_by(&relative, id) {
        return Err(format!("{relative} belongs to {other}: remove it first"));
    }
    let bytes = get(&v.file.url, v.file.size)?;
    let staging = home.join(DOWNLOADS);
    fs::create_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
    let download = staging.join(format!("{id}.hmod.part"));
    fs::write(&download, &bytes).map_err(|e| format!("{}: {e}", download.display()))?;
    let trust = index::check_file(index, p, v, &download).inspect_err(|_| {
        let _ = fs::remove_file(&download);
    })?;
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::rename(&download, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    let verified = trust == Trust::Verified;
    list.record(serde_json::json!({
        "source": "horizon",
        "project": p.id,
        "slug": p.id,
        "title": p.name,
        "kind": "mod",
        "version": v.version,
        "number": v.version,
        "file": relative,
        "checksum": format!("SHA-256:{}", v.file.sha256),
        "page": format!("{SITE}/{}", p.id),
        "dependency": false,
        "verdict": if verified { "Verified" } else { "Unsigned" },
    }));
    list.save(home)?;
    Ok(Done {
        id: id.to_owned(),
        version: v.version.clone(),
        verified,
        pack: false,
    })
}

/// Downloads `url` (at most `max` bytes): `https://` only, or a `file://` path
/// when the index itself is a local test registry.
pub fn download(url: &str, max: u64) -> Result<Vec<u8>, String> {
    if let Some(path) = url.strip_prefix("file://") {
        if !is_local(&location()) {
            return Err(format!("{url} is not an https address"));
        }
        return fs::read(path).map_err(|e| format!("{path}: {e}"));
    }
    if !url.starts_with("https://") {
        return Err(format!("{url} is not an https address"));
    }
    #[cfg(target_os = "emscripten")]
    {
        horizon_system::web_http::get(url, max)
    }
    #[cfg(not(target_os = "emscripten"))]
    {
        let response = ureq::get(url)
            .set("User-Agent", AGENT)
            .call()
            .map_err(|e| e.to_string())?;
        let mut body = Vec::new();
        response
            .into_reader()
            .take(max + 1)
            .read_to_end(&mut body)
            .map_err(|e| e.to_string())?;
        if body.len() as u64 > max {
            return Err(format!("{url} is larger than the index says"));
        }
        Ok(body)
    }
}

/// `mod-browser.json`, kept as JSON values so the launcher's entries this
/// game doesn't model (Modrinth resource packs, ported Java mods) are written
/// back as they were.
#[derive(Debug, Default)]
struct Installed {
    entries: Vec<serde_json::Value>,
}

impl Installed {
    fn load(home: &Path) -> Self {
        let entries = fs::read_to_string(home.join(MANIFEST))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("entries")?.as_array().cloned())
            .unwrap_or_default();
        Self { entries }
    }

    fn save(&self, home: &Path) -> Result<(), String> {
        let path = home.join(MANIFEST);
        let part = home.join(format!(".{MANIFEST}.part"));
        let text = serde_json::to_string_pretty(&serde_json::json!({ "entries": self.entries }))
            .map_err(|e| e.to_string())?;
        fs::write(&part, text).map_err(|e| format!("{}: {e}", part.display()))?;
        fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The registry's entry for project `id`.
    fn position(&self, id: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e["source"] == "horizon" && e["project"] == id)
    }

    /// The title of another entry that owns `file`.
    fn taken_by(&self, file: &str, id: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|e| e["file"] == file && !(e["source"] == "horizon" && e["project"] == id))
            .map(|e| e["title"].as_str().unwrap_or("another mod").to_owned())
    }

    fn record(&mut self, entry: serde_json::Value) {
        let id = entry["project"].clone();
        self.entries
            .retain(|e| !(e["source"] == "horizon" && e["project"] == id));
        self.entries.push(entry);
    }

    /// The version of `id` installed from the registry whose file is still
    /// there; for a `horizon-mods/<id>.hmod` put there another way, its
    /// manifest's version.
    fn version_of(&self, home: &Path, id: &str) -> Option<String> {
        if let Some(e) = self.position(id).map(|i| &self.entries[i])
            && let (Some(version), Some(file)) = (e["version"].as_str(), e["file"].as_str())
            && home.join(file).exists()
        {
            return Some(version.to_owned());
        }
        let path: PathBuf = home.join(crate::mod_list::DIR).join(format!("{id}.hmod"));
        horizon_mods::Package::read(&path)
            .ok()
            .filter(|p| p.manifest.id == id)
            .map(|p| p.manifest.version)
    }
}

#[cfg(test)]
mod tests;

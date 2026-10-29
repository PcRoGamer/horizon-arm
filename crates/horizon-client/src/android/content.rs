//! Android's deliberately narrow content browser backend.
//!
//! * Horizon registry -> sandboxed `.hmod` mods only.
//! * Modrinth -> resource packs only.
//!
//! Java mods, native mixins and Modrinth modpacks are intentionally absent.
//! The normal game path never waits on this module; management UIs call these
//! functions from worker threads.

use horizon_mods::index::{self, Index, Kind as HorizonKind, Project as HorizonProject};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha512};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const HORIZON_INDEX: &str = "https://horizon-rust.github.io/horizon-mods/index.json";
const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const MAX_INDEX: u64 = 16 << 20;
const MAX_RESOURCE_PACK: u64 = 512 << 20;
const MODS_DIR: &str = "horizon-mods";
const MODS_LIST: &str = "horizon-mods.txt";
const CACHE_DIR: &str = "mod-browser";
const HORIZON_CACHE: &str = "horizon-index.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HorizonMod {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub authors: Vec<String>,
    pub version: String,
    pub downloads: u64,
    pub verified: bool,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourcePack {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPack {
    pub project: String,
    pub version: String,
    pub title: String,
    pub file: String,
    pub sha512: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct PackProvenance {
    #[serde(default)]
    modrinth: Vec<InstalledPack>,
}

/// Horizon's reviewed registry, cached per instance for offline browsing.  The
/// index parser validates the schema, ids, URLs, hashes, signatures and pack
/// relationships before callers ever see a project.
pub fn horizon_index(home: &Path) -> Result<Index, String> {
    let cache = home.join(CACHE_DIR).join(HORIZON_CACHE);
    match fetch_text(HORIZON_INDEX, MAX_INDEX) {
        Ok(text) => {
            let parsed = Index::parse(&text)?;
            if let Some(parent) = cache.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&cache, text);
            Ok(parsed)
        }
        Err(network) => {
            let text = fs::read_to_string(&cache).map_err(|_| network)?;
            Index::parse(&text)
        }
    }
}

/// Only `.hmod` registry projects are surfaced. Native mixin bundles need a
/// source rebuild and modpacks are outside the Android product boundary.
pub fn horizon_mods(home: &Path) -> Result<Vec<HorizonMod>, String> {
    let index = horizon_index(home)?;
    let mut out: Vec<_> = index
        .projects
        .iter()
        .filter(|project| project.kind == HorizonKind::Mod)
        .filter_map(|project| {
            let version = index.installable(
                project,
                horizon_core::VANILLA_VERSION,
                horizon_mods::api_level::LEVEL,
            )?;
            Some(HorizonMod {
                id: project.id.clone(),
                name: project.name.clone(),
                summary: project.summary.clone(),
                authors: project.authors.clone(),
                version: version.version.clone(),
                downloads: project.downloads,
                verified: project.key.is_some(),
                enabled: mod_enabled(home, &project.id),
            })
        })
        .collect();
    out.sort_by_key(|project| std::cmp::Reverse(project.downloads));
    Ok(out)
}

/// Install the newest compatible Horizon mod and all registry-pinned
/// dependencies. Every downloaded file passes the registry's full size,
/// SHA-256, revocation, signature and manifest check before reaching the mod
/// folder.
pub fn install_horizon_mod(home: &Path, id: &str) -> Result<Vec<String>, String> {
    let index = horizon_index(home)?;
    let project = index
        .project(id)
        .ok_or_else(|| format!("{id} is not in Horizon's registry."))?;
    if project.kind != HorizonKind::Mod {
        return Err(format!("{} is not a Horizon .hmod mod.", project.name));
    }
    let mut visiting = HashSet::new();
    let mut installed = Vec::new();
    install_horizon_project(&index, project, None, home, &mut visiting, &mut installed)?;
    Ok(installed)
}

fn install_horizon_project(
    index: &Index,
    project: &HorizonProject,
    exact: Option<(&str, &str)>,
    home: &Path,
    visiting: &mut HashSet<String>,
    installed: &mut Vec<String>,
) -> Result<(), String> {
    if project.kind != HorizonKind::Mod {
        return Err(format!("{} is not an installable .hmod dependency.", project.name));
    }
    if !visiting.insert(project.id.clone()) {
        return Err(format!("Dependency cycle at {}.", project.id));
    }

    let version = match exact {
        Some((number, hash)) => project
            .versions
            .iter()
            .find(|version| {
                version.version == number
                    && version.file.sha256 == hash
                    && version.game == horizon_core::VANILLA_VERSION
                    && version
                        .api_level
                        .is_none_or(|level| level <= horizon_mods::api_level::LEVEL)
                    && index.revoked(&project.id, &version.file.sha256).is_none()
            }),
        None => index.installable(
            project,
            horizon_core::VANILLA_VERSION,
            horizon_mods::api_level::LEVEL,
        ),
    }
    .ok_or_else(|| format!("{} has no compatible version for this Horizon.", project.name))?;

    for requirement in &version.requires {
        let dependency = index.project(&requirement.id).ok_or_else(|| {
            format!(
                "{} needs {} {}, which is missing from the registry.",
                project.name, requirement.id, requirement.version
            )
        })?;
        install_horizon_project(
            index,
            dependency,
            Some((&requirement.version, &requirement.sha256)),
            home,
            visiting,
            installed,
        )?;
    }

    let target = home.join(MODS_DIR).join(format!("{}.hmod", project.id));
    let already = target.is_file()
        && index::sha256_file(&target).is_ok_and(|hash| hash == version.file.sha256);
    if !already {
        fs::create_dir_all(home.join(MODS_DIR))
            .map_err(|error| format!("{}: {error}", home.join(MODS_DIR).display()))?;
        let staging = home.join(CACHE_DIR).join("downloads");
        fs::create_dir_all(&staging)
            .map_err(|error| format!("{}: {error}", staging.display()))?;
        let path = staging.join(format!("{}.hmod.part", project.id));
        fetch_file(&version.file.url, &path, version.file.size, index::MAX_FILE)?;
        index::check_file(index, project, version, &path).inspect_err(|_| {
            let _ = fs::remove_file(&path);
        })?;
        let incoming = target.with_extension("hmod.incoming");
        let _ = fs::remove_file(&incoming);
        fs::rename(&path, &incoming)
            .map_err(|error| format!("{}: {error}", incoming.display()))?;
        let _ = fs::remove_file(&target);
        fs::rename(&incoming, &target)
            .map_err(|error| format!("{}: {error}", target.display()))?;
        installed.push(format!("{} {}", project.name, version.version));
    }
    set_mod_enabled(home, &project.id, true)?;
    visiting.remove(&project.id);
    Ok(())
}

pub fn remove_horizon_mod(home: &Path, id: &str) -> Result<(), String> {
    if !index::valid_id(id) {
        return Err("Invalid Horizon mod id.".into());
    }
    set_mod_enabled(home, id, false)?;
    let target = home.join(MODS_DIR).join(format!("{id}.hmod"));
    if target.exists() {
        fs::remove_file(&target).map_err(|error| format!("{}: {error}", target.display()))?;
    }
    Ok(())
}

pub fn mod_enabled(home: &Path, id: &str) -> bool {
    let key = format!("mod.{id}");
    fs::read_to_string(home.join(MODS_LIST))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once(':'))
        .any(|(name, value)| name.trim() == key && value.trim() == "true")
}

pub fn set_mod_enabled(home: &Path, id: &str, enabled: bool) -> Result<(), String> {
    if !index::valid_id(id) {
        return Err("Invalid Horizon mod id.".into());
    }
    let path = home.join(MODS_LIST);
    let key = format!("mod.{id}");
    let mut lines: Vec<String> = fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.split_once(':').is_none_or(|(name, _)| name.trim() != key))
        .map(str::to_owned)
        .collect();
    if enabled {
        lines.push(format!("{key}:true"));
    }
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Search Modrinth with a hard `project_type:resourcepack` facet. No caller can
/// use this backend to discover mods or modpacks accidentally.
pub fn search_modrinth_resource_packs(query: &str, offset: usize) -> Result<Vec<ResourcePack>, String> {
    let facets = serde_json::to_string(&vec![
        vec!["project_type:resourcepack"],
        vec![&format!("versions:{}", horizon_core::VANILLA_VERSION)],
    ])
    .map_err(|error| error.to_string())?;
    let mut url = format!(
        "{MODRINTH_API}/search?facets={}&index=downloads&offset={}&limit=20",
        encode(&facets),
        offset
    );
    if !query.trim().is_empty() {
        url.push_str("&query=");
        url.push_str(&encode(query.trim()));
    }
    let json = fetch_json(&url, 4 << 20)?;
    Ok(json["hits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|hit| hit["project_type"].as_str() == Some("resourcepack"))
        .filter_map(|hit| {
            Some(ResourcePack {
                id: hit["project_id"].as_str()?.to_owned(),
                slug: hit["slug"].as_str()?.to_owned(),
                title: hit["title"].as_str()?.to_owned(),
                description: hit["description"].as_str().unwrap_or_default().to_owned(),
                author: hit["author"].as_str().unwrap_or_default().to_owned(),
                downloads: hit["downloads"].as_u64().unwrap_or(0),
                icon_url: hit["icon_url"].as_str().map(str::to_owned),
            })
        })
        .collect())
}

/// Download the newest Modrinth resource-pack version for Horizon's Minecraft
/// version, verify Modrinth's declared size and hashes, then validate that it is
/// actually a well-formed Minecraft resource-pack ZIP before installing it.
pub fn install_modrinth_resource_pack(home: &Path, project_id: &str) -> Result<InstalledPack, String> {
    let project_url = format!("{MODRINTH_API}/project/{}", encode(project_id));
    let project = fetch_json(&project_url, 2 << 20)?;
    if project["project_type"].as_str() != Some("resourcepack") {
        return Err("That Modrinth project is not a resource pack.".into());
    }
    let title = project["title"].as_str().unwrap_or(project_id).to_owned();

    let versions_url = format!("{MODRINTH_API}/project/{}/version", encode(project_id));
    let versions = fetch_json(&versions_url, 8 << 20)?;
    let version = versions
        .as_array()
        .into_iter()
        .flatten()
        .find(|version| {
            version["status"].as_str().is_none_or(|status| status == "listed")
                && version["game_versions"]
                    .as_array()
                    .is_some_and(|versions| {
                        versions.iter().any(|game| {
                            game.as_str() == Some(horizon_core::VANILLA_VERSION)
                        })
                    })
        })
        .ok_or_else(|| format!("{title} has no resource-pack version for this Minecraft."))?;
    let version_number = version["version_number"].as_str().unwrap_or_default().to_owned();
    let files = version["files"]
        .as_array()
        .ok_or_else(|| format!("{title}: Modrinth returned no files."))?;
    let file = files
        .iter()
        .find(|file| file["primary"].as_bool() == Some(true))
        .or_else(|| files.first())
        .ok_or_else(|| format!("{title}: Modrinth returned no download."))?;
    let url = file["url"]
        .as_str()
        .filter(|url| url.starts_with("https://"))
        .ok_or_else(|| format!("{title}: invalid download URL."))?;
    let filename = safe_zip_name(file["filename"].as_str().unwrap_or_default())?;
    let size = file["size"]
        .as_u64()
        .filter(|size| *size > 0 && *size <= MAX_RESOURCE_PACK)
        .ok_or_else(|| format!("{title}: invalid download size."))?;
    let expected_512 = file["hashes"]["sha512"]
        .as_str()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| format!("{title}: Modrinth supplied no SHA-512."))?;
    let expected_1 = file["hashes"]["sha1"]
        .as_str()
        .map(str::to_ascii_lowercase);

    let staging = home.join(CACHE_DIR).join("downloads");
    fs::create_dir_all(&staging)
        .map_err(|error| format!("{}: {error}", staging.display()))?;
    let part = staging.join(format!("{project_id}.resourcepack.part"));
    fetch_file(url, &part, size, MAX_RESOURCE_PACK)?;
    let bytes = fs::read(&part).map_err(|error| format!("{}: {error}", part.display()))?;
    let actual_512 = hex(&Sha512::digest(&bytes));
    if actual_512 != expected_512 {
        let _ = fs::remove_file(&part);
        return Err(format!("{title}: SHA-512 did not match Modrinth."));
    }
    if let Some(expected) = expected_1 {
        let actual = hex(&Sha1::digest(&bytes));
        if actual != expected {
            let _ = fs::remove_file(&part);
            return Err(format!("{title}: SHA-1 did not match Modrinth."));
        }
    }
    horizon_mods::index::inspect(&HorizonKind::ResourcePack, &part).inspect_err(|_| {
        let _ = fs::remove_file(&part);
    })?;

    let folder = home.join("resourcepacks");
    fs::create_dir_all(&folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    let target = folder.join(&filename);
    let incoming = folder.join(format!(".{filename}.incoming"));
    let _ = fs::remove_file(&incoming);
    fs::rename(&part, &incoming).map_err(|error| format!("{}: {error}", incoming.display()))?;
    let _ = fs::remove_file(&target);
    fs::rename(&incoming, &target).map_err(|error| format!("{}: {error}", target.display()))?;

    let installed = InstalledPack {
        project: project_id.to_owned(),
        version: version_number,
        title,
        file: filename,
        sha512: expected_512,
    };
    save_pack_provenance(home, installed.clone())?;
    Ok(installed)
}

pub fn installed_modrinth_packs(home: &Path) -> Vec<InstalledPack> {
    let mut provenance = load_pack_provenance(home);
    provenance
        .modrinth
        .retain(|pack| home.join("resourcepacks").join(&pack.file).is_file());
    provenance.modrinth
}

pub fn remove_modrinth_resource_pack(home: &Path, project: &str) -> Result<(), String> {
    let mut provenance = load_pack_provenance(home);
    if let Some(pack) = provenance.modrinth.iter().find(|pack| pack.project == project).cloned() {
        let path = home.join("resourcepacks").join(&pack.file);
        if path.is_file() {
            fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        }
        provenance.modrinth.retain(|entry| entry.project != project);
        write_pack_provenance(home, &provenance)?;
    }
    Ok(())
}

fn load_pack_provenance(home: &Path) -> PackProvenance {
    fs::read_to_string(home.join(CACHE_DIR).join("android-content.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_pack_provenance(home: &Path, pack: InstalledPack) -> Result<(), String> {
    let mut provenance = load_pack_provenance(home);
    provenance.modrinth.retain(|entry| entry.project != pack.project);
    provenance.modrinth.push(pack);
    write_pack_provenance(home, &provenance)
}

fn write_pack_provenance(home: &Path, provenance: &PackProvenance) -> Result<(), String> {
    let path = home.join(CACHE_DIR).join("android-content.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(provenance).map_err(|error| error.to_string())?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

fn fetch_text(url: &str, max: u64) -> Result<String, String> {
    let response = ureq::get(url)
        .set("User-Agent", "Horizon-Android/0.1")
        .call()
        .map_err(|error| format!("{url}: {error}"))?;
    let mut text = String::new();
    response
        .into_reader()
        .take(max + 1)
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    if text.len() as u64 > max {
        return Err(format!("{url}: response is too large."));
    }
    Ok(text)
}

fn fetch_json(url: &str, max: u64) -> Result<serde_json::Value, String> {
    serde_json::from_str(&fetch_text(url, max)?).map_err(|error| format!("{url}: {error}"))
}

fn fetch_file(url: &str, path: &Path, expected: u64, max: u64) -> Result<(), String> {
    if !url.starts_with("https://") || expected == 0 || expected > max {
        return Err("Refusing an invalid or oversized content download.".into());
    }
    let response = ureq::get(url)
        .set("User-Agent", "Horizon-Android/0.1")
        .call()
        .map_err(|error| format!("{url}: {error}"))?;
    let mut reader = response.into_reader().take(max + 1);
    let mut file = fs::File::create(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let copied = std::io::copy(&mut reader, &mut file).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    if copied != expected {
        let _ = fs::remove_file(path);
        return Err(format!("Download was {copied} bytes; expected {expected}."));
    }
    Ok(())
}

fn safe_zip_name(name: &str) -> Result<String, String> {
    let safe = !name.is_empty()
        && name.len() <= 160
        && name.ends_with(".zip")
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-+() []".contains(&byte));
    safe.then(|| name.to_owned())
        .ok_or_else(|| "Modrinth returned an unsafe resource-pack file name.".into())
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

//! Launcherless Android instances shared by the activity shell and game UI.
//!
//! The original game folder remains the default `Horizon` instance. New
//! instances live under `instances/<slug>/`; display metadata is separate, so
//! renaming an instance never renames or moves its folder. There is deliberately
//! no version field: Android has one Horizon runtime and many instances.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const DEFAULT: &str = "horizon";
const FILE: &str = "instances.json";
const SELECTED: &str = "selected-instance";
const OWN: &str = "instances";
const TRASH: &str = "instance-trash";
const PLAYER_FILES: &[&str] = &["options.txt", "horizon-player.txt"];
const NOT_COPIED: &[&str] = &[
    OWN,
    TRASH,
    FILE,
    SELECTED,
    ".local",
    "horizon-client",
    "resources",
    "ports",
    "horizon-packs",
    "logs",
    "crash-reports",
    "sessions",
    "telemetry",
    "mod-browser",
    ".horizon-cache",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Default,
    Own,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Icon {
    Texture(String),
    Colour(String),
}

impl Icon {
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if let Some(hex) = text.strip_prefix('#') {
            if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Ok(Self::Colour(format!("#{}", hex.to_ascii_lowercase())));
            }
        } else if !text.is_empty()
            && text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Ok(Self::Texture(text.to_owned()));
        }
        Err(format!(
            "{text}: give a block or item name (grass_block) or a colour (#3fa7d6)"
        ))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub played_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played: Option<u64>,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    instances: BTreeMap<String, Meta>,
}

impl Store {
    fn load(root: &Path) -> Self {
        fs::read_to_string(root.join(FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self, root: &Path) -> Result<(), String> {
        fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        atomic_write(&root.join(FILE), text.as_bytes())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub home: PathBuf,
    pub icon: Option<Icon>,
    pub group: Option<String>,
    pub played: Duration,
    pub last_played: Option<SystemTime>,
}

pub fn root() -> Option<PathBuf> {
    std::env::var_os("HORIZON_ANDROID_ROOT")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn current_id() -> String {
    std::env::var("HORIZON_INSTANCE_ID").unwrap_or_else(|_| DEFAULT.into())
}

pub fn ensure(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root.join(OWN)).map_err(|e| format!("instances: {e}"))?;
    if !root.join(SELECTED).exists() {
        atomic_write(&root.join(SELECTED), DEFAULT.as_bytes())?;
    }
    Ok(())
}

pub fn list(root: &Path) -> Vec<Instance> {
    let mut store = Store::load(root);
    let mut found = vec![(
        DEFAULT.to_owned(),
        "Horizon".to_owned(),
        Kind::Default,
        root.to_path_buf(),
    )];

    let own_dir = root.join(OWN);
    let mut own: Vec<String> = fs::read_dir(&own_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    own.sort();

    if own_dir.is_dir() {
        let before = store.instances.len();
        store
            .instances
            .retain(|id, _| id == DEFAULT || own.iter().any(|own| own == id));
        if store.instances.len() != before {
            let _ = store.save(root);
        }
    }

    for slug in own {
        found.push((
            slug.clone(),
            slug.clone(),
            Kind::Own,
            own_dir.join(slug),
        ));
    }

    let mut out: Vec<_> = found
        .into_iter()
        .map(|(id, fallback_name, kind, home)| {
            let meta = store.instances.get(&id).cloned().unwrap_or_default();
            Instance {
                id,
                name: meta.name.unwrap_or(fallback_name),
                kind,
                home,
                icon: meta.icon,
                group: meta.group,
                played: Duration::from_secs(meta.played_secs),
                last_played: meta
                    .last_played
                    .map(|secs| UNIX_EPOCH + Duration::from_secs(secs)),
            }
        })
        .collect();
    if out.len() > 1 {
        out[1..].sort_by_key(|instance| {
            (instance.group.clone(), instance.name.to_lowercase())
        });
    }
    out
}

pub fn find(root: &Path, query: &str) -> Result<Instance, String> {
    let all = list(root);
    let query = query.trim();
    all.iter()
        .find(|instance| instance.id == query)
        .or_else(|| {
            all.iter()
                .find(|instance| instance.name.eq_ignore_ascii_case(query))
        })
        .cloned()
        .ok_or_else(|| format!("No instance {query}"))
}

pub fn selected(root: &Path) -> Instance {
    let all = list(root);
    let id = fs::read_to_string(root.join(SELECTED))
        .unwrap_or_default()
        .trim()
        .to_owned();
    all.iter()
        .find(|instance| instance.id == id)
        .cloned()
        .unwrap_or_else(|| all[0].clone())
}

pub fn select(root: &Path, query: &str) -> Result<Instance, String> {
    let instance = find(root, query)?;
    atomic_write(&root.join(SELECTED), instance.id.as_bytes())?;
    Ok(instance)
}

pub fn create(root: &Path, name: &str, from: Option<&str>) -> Result<Instance, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("An instance needs a name.".into());
    }
    let all = list(root);
    if all
        .iter()
        .any(|instance| instance.name.eq_ignore_ascii_case(name))
    {
        return Err(format!("There is already an instance called {name}."));
    }
    let source = from.map(|query| find(root, query)).transpose()?;
    let base = slug(name);
    let dir_root = root.join(OWN);
    let id = (1..)
        .map(|n| {
            if n == 1 {
                base.clone()
            } else {
                format!("{base}-{n}")
            }
        })
        .find(|candidate| candidate != DEFAULT && !dir_root.join(candidate).exists())
        .expect("unbounded unique instance suffix");
    let dir = dir_root.join(&id);

    let made = match &source {
        Some(source) => copy_home(&source.home, &dir),
        None => fs::create_dir_all(&dir).and_then(|()| {
            for file in PLAYER_FILES {
                let source = root.join(file);
                if source.is_file() {
                    fs::copy(&source, dir.join(file))?;
                }
            }
            Ok(())
        }),
    };
    if let Err(error) = made {
        let _ = fs::remove_dir_all(&dir);
        return Err(format!("Could not make {}: {error}", dir.display()));
    }

    let mut store = Store::load(root);
    let meta = store.instances.entry(id.clone()).or_default();
    meta.name = (name != id).then(|| name.to_owned());
    meta.icon = source.as_ref().and_then(|instance| instance.icon.clone());
    meta.group = source.as_ref().and_then(|instance| instance.group.clone());
    store.save(root)?;
    find(root, &id)
}

pub fn rename(root: &Path, query: &str, name: &str) -> Result<Instance, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("An instance needs a name.".into());
    }
    let all = list(root);
    let instance = find(root, query)?;
    if all.iter().any(|other| {
        other.id != instance.id && other.name.eq_ignore_ascii_case(name)
    }) {
        return Err(format!("There is already an instance called {name}."));
    }
    let mut store = Store::load(root);
    let meta = store.instances.entry(instance.id.clone()).or_default();
    meta.name = if (instance.kind == Kind::Default && name == "Horizon") || name == instance.id {
        None
    } else {
        Some(name.to_owned())
    };
    store.save(root)?;
    find(root, &instance.id)
}

pub fn set_group(root: &Path, query: &str, group: Option<&str>) -> Result<Instance, String> {
    let instance = find(root, query)?;
    let mut store = Store::load(root);
    store.instances.entry(instance.id.clone()).or_default().group = group
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .map(str::to_owned);
    store.save(root)?;
    find(root, &instance.id)
}

pub fn set_icon(root: &Path, query: &str, icon: Option<Icon>) -> Result<Instance, String> {
    let instance = find(root, query)?;
    let mut store = Store::load(root);
    store.instances.entry(instance.id.clone()).or_default().icon = icon;
    store.save(root)?;
    find(root, &instance.id)
}

pub fn delete(root: &Path, query: &str) -> Result<Instance, String> {
    let instance = find(root, query)?;
    if instance.kind == Kind::Default {
        return Err("The default Horizon instance cannot be deleted.".into());
    }
    let was_selected = fs::read_to_string(root.join(SELECTED))
        .is_ok_and(|id| id.trim() == instance.id);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let trash = root.join(TRASH);
    fs::create_dir_all(&trash).map_err(|e| format!("{}: {e}", trash.display()))?;
    let target = trash.join(format!("{}-{stamp}", instance.id));
    fs::rename(&instance.home, &target)
        .map_err(|e| format!("Could not move {} to recovery: {e}", instance.name))?;

    let mut store = Store::load(root);
    store.instances.remove(&instance.id);
    store.save(root)?;
    if was_selected {
        atomic_write(&root.join(SELECTED), DEFAULT.as_bytes())?;
    }
    Ok(instance)
}

pub fn record_play(root: &Path, id: &str, played: Duration) -> Result<(), String> {
    let mut store = Store::load(root);
    let meta = store.instances.entry(id.to_owned()).or_default();
    meta.played_secs = meta.played_secs.saturating_add(played.as_secs());
    meta.last_played = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs());
    store.save(root)
}

pub fn follow_player(root: &Path, instance: &Instance) {
    if instance.kind == Kind::Default {
        return;
    }
    let _ = fs::create_dir_all(&instance.home);
    for file in PLAYER_FILES {
        let from = root.join(file);
        let to = instance.home.join(file);
        if !to.exists() && from.is_file() {
            let _ = fs::copy(from, to);
        }
    }
}

pub fn cycle(root: &Path, forward: bool) -> Result<Instance, String> {
    let all = list(root);
    if all.is_empty() {
        return Err("No instances".into());
    }
    let current = selected(root);
    let index = all
        .iter()
        .position(|instance| instance.id == current.id)
        .unwrap_or(0);
    let next = if forward {
        (index + 1) % all.len()
    } else if index == 0 {
        all.len() - 1
    } else {
        index - 1
    };
    select(root, &all[next].id)
}

fn copy_home(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if NOT_COPIED.iter().any(|skip| name == *skip) {
            continue;
        }
        let source = entry.path();
        let target = to.join(&name);
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&source, &target)?;
        } else if kind.is_file() {
            fs::copy(source, target)?;
        }
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&source, &target)?;
        } else if kind.is_file() {
            fs::copy(source, target)?;
        }
    }
    Ok(())
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut separator = false;
    for ch in name.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            if separator && !out.is_empty() {
                out.push('-');
            }
            separator = false;
            out.push(ch);
        } else if !out.is_empty() {
            separator = true;
        }
    }
    let out = out.trim_matches('-').to_owned();
    if out.is_empty() {
        "instance".into()
    } else {
        out
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

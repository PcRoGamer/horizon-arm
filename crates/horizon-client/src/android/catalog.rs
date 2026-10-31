//! Mobile launcher state. Browser operations reuse the desktop backend unchanged.
use crate::browser::{self, Kind, Project, Query, Source, installed::Installed};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};

/// Real Vulkan rendering paths; these do not select a different driver or API.
#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RendererMode {
    Vulkan,
    #[default]
    VulkanCompatibility,
}
impl RendererMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Vulkan => "Vulkan / GPU batching",
            Self::VulkanCompatibility => "Vulkan / compatibility",
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub profile: u32,
    pub name: String,
    pub server: String,
    pub pack: Option<String>,
    pub renderer: RendererMode,
}
impl Config {
    pub fn load(home: &Path) -> Self {
        let value: serde_json::Value = std::fs::read(home.join("android-launcher.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut config: Self = serde_json::from_value(value.clone()).unwrap_or_default();
        if value.get("renderer").is_none() && value["gpu_batching"].as_bool() == Some(true) {
            config.renderer = RendererMode::Vulkan;
        }
        config
    }
    pub fn save(&self, home: &Path) -> Result<(), String> {
        let part = home.join("android-launcher.partial");
        std::fs::write(
            &part,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(part, home.join("android-launcher.json")).map_err(|e| e.to_string())
    }
    pub fn profile_home(&self, home: &Path) -> PathBuf {
        if self.profile == 0 {
            home.into()
        } else {
            home.join("profiles")
                .join(format!("profile-{}", self.profile))
        }
    }
    pub fn game_home(&self, home: &Path) -> PathBuf {
        let base = self.profile_home(home);
        self.pack
            .as_deref()
            .filter(|id| browser::safe_name(id).is_some())
            .map_or(base.clone(), |id| {
                base.join("ported-packs").join(id).join("home")
            })
    }
    pub fn pack_mods(&self, home: &Path) -> Vec<PathBuf> {
        let Some(id) = self
            .pack
            .as_deref()
            .filter(|id| browser::safe_name(id).is_some())
        else {
            return vec![];
        };
        let work = self.profile_home(home).join("ported-packs").join(id);
        let mut mods: Vec<_> = std::fs::read_dir(work.join("mods"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path().join("carry"))
            .filter(|p| p.join("horizon-mod.toml").is_file())
            .collect();
        mods.sort();
        horizon_port_pack::pipeline::loadable(&mods).0
    }
}
#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Play,
    Mods,
    Packs,
    Modpacks,
    Installed,
    Settings,
}
impl Page {
    pub const ALL: [Self; 6] = [
        Self::Play,
        Self::Mods,
        Self::Packs,
        Self::Modpacks,
        Self::Installed,
        Self::Settings,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::Mods => "Mods",
            Self::Packs => "Packs",
            Self::Modpacks => "Modpacks",
            Self::Installed => "Installed",
            Self::Settings => "Settings",
        }
    }
    pub fn kind(self) -> Kind {
        match self {
            Self::Packs => Kind::ResourcePack,
            Self::Modpacks => Kind::Modpack,
            _ => Kind::Mod,
        }
    }
    pub fn is_catalog(self) -> bool {
        matches!(self, Self::Mods | Self::Packs | Self::Modpacks)
    }
}
enum Reply {
    Search(Vec<Project>),
    Done(String),
}
pub struct Catalog {
    pub native: Vec<(String, String, bool)>,
    pub page: Page,
    pub query: String,
    pub results: Vec<Project>,
    pub status: String,
    pub offset: usize,
    pub scroll: f32,
    pub installed: Installed,
    pub config: Config,
    pub enabled: Vec<bool>,
    pub editing: Option<&'static str>,
    worker: Option<mpsc::Receiver<Result<Reply, String>>>,
}
impl Catalog {
    pub fn new(home: &Path) -> Self {
        let config = Config::load(home);
        let installed = Installed::load(&config.game_home(home));
        let mut result = Self {
            native: vec![],
            enabled: vec![],
            page: Page::Play,
            query: String::new(),
            results: vec![],
            status: String::new(),
            offset: 0,
            scroll: 0.0,
            installed,
            config,
            editing: None,
            worker: None,
        };
        result.refresh(home);
        result
    }
    pub fn refresh(&mut self, home: &Path) {
        let game = self.config.game_home(home);
        self.installed = Installed::load(&game);
        let on = horizon_client::mod_list::ModList::load(&game);
        let settings = horizon_client::settings::Settings::load(&game.join("options.txt"));
        self.enabled = self
            .installed
            .entries
            .iter()
            .map(|e| match e.kind {
                Kind::Mod => e
                    .file
                    .strip_prefix("horizon-mods/")
                    .is_some_and(|id| on.enabled.contains(id)),
                Kind::ResourcePack => settings.resource_packs.contains(&format!(
                    "file/{}",
                    Path::new(&e.file)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )),
                _ => false,
            })
            .collect();
        self.native = horizon_mods::find(&game.join("horizon-mods"))
            .into_iter()
            .filter(|p| p.is_file())
            .filter_map(|p| horizon_mods::Package::read(&p).ok())
            .map(|p| {
                let m = p.manifest;
                let enabled = on.enabled.contains(&m.id);
                (m.id, m.name, enabled)
            })
            .collect();
    }
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    fn job(&mut self, status: &str, job: impl FnOnce() -> Result<Reply, String> + Send + 'static) {
        if self.busy() {
            return;
        }
        self.status = status.into();
        let (tx, rx) = mpsc::channel();
        self.worker = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(job());
        });
    }
    pub fn poll(&mut self, home: &Path) {
        let reply = self.worker.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(v) => Some(v),
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("Launcher worker stopped".into())),
            Err(_) => None,
        });
        if let Some(reply) = reply {
            self.worker = None;
            match reply {
                Ok(Reply::Search(items)) => {
                    self.status = format!(
                        "{} results - select Install to check compatibility",
                        items.len()
                    );
                    self.results = items;
                }
                Ok(Reply::Done(s)) => self.status = s,
                Err(e) => self.status = e,
            }
            self.refresh(home);
        }
    }
    pub fn search(&mut self, home: &Path) {
        if self.page == Page::Mods && self.query.starts_with("https://") {
            self.import_native(home);
            return;
        }
        let mut query = Query::new(&self.query, self.page.kind());
        query.everything = true;
        query.offset = self.offset;
        query.limit = 12;
        self.scroll = 0.0;
        self.job("Searching Modrinth...", move || {
            browser::modrinth::Modrinth
                .search(&query)
                .map(Reply::Search)
        });
    }
    pub fn import_native(&mut self, home: &Path) {
        let url = self.query.clone();
        let game = self.config.game_home(home);
        self.job("Downloading native .hmod over HTTPS...",move||{
            use std::io::Read;
            let mut bytes=Vec::new();
            ureq::AgentBuilder::new().https_only(true).timeout_connect(std::time::Duration::from_secs(20)).timeout_read(std::time::Duration::from_secs(90)).build().get(&url).call().map_err(|e|e.to_string())?.into_reader().take(32*1024*1024+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
            if bytes.len()>32*1024*1024 {return Err("Native mod exceeds 32 MiB".into());}
            let dir=game.join("horizon-mods");std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
            let stage=dir.join("import.partial");std::fs::write(&stage,bytes).map_err(|e|e.to_string())?;
            let result=(||{
                let package=horizon_mods::Package::read(&stage)?;
                let destination=dir.join(format!("{}.hmod",package.manifest.id));
                if destination.exists(){std::fs::copy(&destination,destination.with_extension("hmod.previous")).map_err(|e|e.to_string())?;}
                std::fs::rename(&stage,destination).map_err(|e|e.to_string())?;
                Ok(Reply::Done(format!("Installed {}. Enable it in Installed after reviewing its capabilities in game Mods.",package.manifest.name)))
            })();
            let _=std::fs::remove_file(stage);result
        });
    }
    pub fn toggle_native(&mut self, home: &Path, index: usize) {
        let Some((id, _, enabled)) = self.native.get(index) else {
            return;
        };
        let game = self.config.game_home(home);
        let mut list = horizon_client::mod_list::ModList::load(&game);
        if *enabled {
            list.enabled.remove(id);
        } else {
            list.enabled.insert(id.clone());
        }
        list.save(&game);
        self.refresh(home);
    }
    pub fn install(&mut self, home: &Path, index: usize) {
        let Some(project) = self.results.get(index).cloned() else {
            return;
        };
        let game = self.config.game_home(home);
        let resources = home.join(".local/horizon-client/26.3");
        self.job("Checking compatibility and downloading...", move || {
            std::fs::create_dir_all(&game).map_err(|e| e.to_string())?;
            let source = browser::modrinth::Modrinth;
            if project.kind == Kind::Modpack {
                let file = browser::installed::fetch_modpack(
                    &source,
                    &project,
                    &game.join("mod-browser/downloads"),
                    &|_| {},
                )?;
                let work = game.join("ported-packs").join(&project.id);
                use horizon_port_pack::pipeline;
                pipeline::import(&file, &work, &|s| log::info!("{s}"))?;
                pipeline::inventory(&work, false, &|s| log::info!("{s}"))?;
                pipeline::port(&work, Some(&resources), None, &|s| log::info!("{s}"))?;
                // A modpack is an independent profile; never overlay the current saves/options.
                let rows = pipeline::report(&work, &|s| log::info!("{s}"))?;
                return Ok(Reply::Done(format!(
                    "Pack imported: {} entries. Data-only port; Java code cannot run.",
                    rows.len()
                )));
            }
            let entries =
                browser::installed::install(&source, &project, true, &game, &|name, p| {
                    log::info!("{name}: {:.0}%", p * 100.0)
                })?;
            Ok(Reply::Done(format!(
                "Installed {} item(s). Open Play to load this profile.",
                entries.len()
            )))
        });
    }
    pub fn remove(&mut self, home: &Path, index: usize) {
        if self.busy() {
            return;
        }
        let Some(entry) = self.installed.entries.get(index) else {
            return;
        };
        let game = self.config.game_home(home);
        self.status = match browser::installed::remove(&game, &entry.project) {
            Ok(e) => {
                if e.kind == Kind::ResourcePack {
                    let path = game.join("options.txt");
                    let mut s = horizon_client::settings::Settings::load(&path);
                    let name = format!(
                        "file/{}",
                        Path::new(&e.file)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    );
                    s.resource_packs.retain(|p| p != &name);
                    s.save(&path);
                }
                format!("Removed {}", e.title)
            }
            Err(e) => e,
        };
        self.installed = Installed::load(&game);
    }
    pub fn toggle(&mut self, home: &Path, index: usize) {
        let Some(entry) = self.installed.entries.get(index) else {
            return;
        };
        let game = self.config.game_home(home);
        if entry.kind == Kind::Mod {
            let mods = horizon_client::mod_list::ModList::load(&game);
            if let Some(id) = entry.file.strip_prefix("horizon-mods/")
                && let Err(e) = browser::java::set_enabled(&game, id, !mods.enabled.contains(id))
            {
                self.status = e;
                return;
            }
        } else if entry.kind == Kind::ResourcePack {
            let path = game.join("options.txt");
            let mut settings = horizon_client::settings::Settings::load(&path);
            let name = format!(
                "file/{}",
                Path::new(&entry.file)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
            if settings.resource_packs.contains(&name) {
                settings.resource_packs.retain(|p| p != &name);
            } else {
                settings.resource_packs.push(name);
            }
            settings.save(&path);
        }
        self.status = format!("Updated {} for next launch", entry.title);
        self.refresh(home);
    }
    pub fn edit(&mut self, out: horizon_client::osk::Output, home: &Path) {
        use horizon_client::osk::Output;
        let field = match self.editing {
            Some("name") => &mut self.config.name,
            Some("server") => &mut self.config.server,
            _ => &mut self.query,
        };
        match out {
            Output::Text(s) => field.push_str(&s),
            Output::Backspace => {
                field.pop();
            }
            Output::Enter | Output::Close => {
                self.editing = None;
                let _ = self.config.save(home);
                if self.page.is_catalog() {
                    self.offset = 0;
                    self.search(home);
                }
            }
        }
    }
    pub fn pack_ids(&self, home: &Path) -> Vec<String> {
        let mut ids: Vec<String> =
            std::fs::read_dir(self.config.profile_home(home).join("ported-packs"))
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.path().join("pack.json").is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
        ids.sort();
        ids
    }
    pub fn select_pack(&mut self, home: &Path, pack: Option<String>) -> Result<(), String> {
        self.config.pack = pack;
        if self.config.pack.is_some() && self.config.pack_mods(home).is_empty() {
            self.status =
                "This pack has no loadable content yet; unsupported Java behavior cannot run."
                    .into();
            self.config.pack = None;
        }
        std::fs::create_dir_all(self.config.game_home(home)).map_err(|e| e.to_string())?;
        self.refresh(home);
        self.config.save(home)
    }
}

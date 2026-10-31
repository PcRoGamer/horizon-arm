//! First-run resource bootstrap for Android.
//!
//! The APK intentionally contains no Minecraft game assets. It contains only
//! Horizon-generated engine tables (`engine-data.zip`). The pinned client JAR,
//! asset index and game objects are fetched directly from Mojang, verified, and
//! installed atomically into Horizon's ordinary prepared-resource directory.

use horizon_client::Result;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};
use winit::platform::android::activity::AndroidApp;

const CATALOG: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const LOCK: &str = include_str!("../../../research-lock.json");
const MAX_METADATA: u64 = 16 * 1024 * 1024;
const MAX_DOWNLOAD: u64 = 1024 * 1024 * 1024;

pub fn resource_root(home: &Path) -> PathBuf {
    home.join(".local/horizon-client")
        .join(horizon_core::VANILLA_VERSION)
}

pub fn ready(home: &Path) -> bool {
    let root = resource_root(home);
    root.join("horizon-resources.json").is_file()
        && root.join("assets/minecraft").is_dir()
        && root.join("asset-index.json").is_file()
}

fn official_url(text: &str) -> Result<()> {
    let url = url::Url::parse(text)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|p| p != 443)
        || !matches!(
            url.host_str(),
            Some(
                "piston-meta.mojang.com"
                    | "piston-data.mojang.com"
                    | "resources.download.minecraft.net"
                    | "launcher.mojang.com"
                    | "launchermeta.mojang.com"
            )
        )
    {
        return Err("resource URL is not an official Mojang HTTPS endpoint".into());
    }
    Ok(())
}

fn agent() -> ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT
        .get_or_init(|| {
            ureq::AgentBuilder::new()
                .redirects(0)
                .timeout_connect(std::time::Duration::from_secs(30))
                .timeout_read(std::time::Duration::from_secs(120))
                .build()
        })
        .clone()
}

fn get_bytes(url: &str, limit: u64) -> Result<Vec<u8>> {
    official_url(url)?;
    let mut bytes = Vec::new();
    agent()
        .get(url)
        .call()?
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("Mojang response exceeds safety limit".into());
    }
    Ok(bytes)
}

fn json(url: &str) -> Result<Value> {
    Ok(serde_json::from_slice(&get_bytes(url, MAX_METADATA)?)?)
}

fn sha1_path(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[derive(Clone)]
struct Download {
    url: String,
    sha1: String,
    size: u64,
    path: PathBuf,
}

impl Download {
    fn from(value: &Value, path: PathBuf) -> Result<Self> {
        let file = Self {
            url: value["url"].as_str().ok_or("missing download URL")?.into(),
            sha1: value["sha1"].as_str().ok_or("missing download hash")?.into(),
            size: value["size"].as_u64().ok_or("missing download size")?,
            path,
        };
        official_url(&file.url)?;
        if file.sha1.len() != 40
            || !file.sha1.bytes().all(|c| c.is_ascii_hexdigit())
            || file.size > MAX_DOWNLOAD
        {
            return Err("invalid Mojang download metadata".into());
        }
        Ok(file)
    }

    fn get(&self) -> Result<()> {
        if self.path.is_file()
            && fs::metadata(&self.path)?.len() == self.size
            && sha1_path(&self.path)? == self.sha1
        {
            return Ok(());
        }
        fs::create_dir_all(self.path.parent().ok_or("missing download directory")?)?;
        let partial = self.path.with_extension("partial");
        for attempt in 0..3 {
            let result = (|| -> Result<()> {
                let mut input = agent()
                    .get(&self.url)
                    .call()?
                    .into_reader()
                    .take(self.size + 1);
                let mut output = fs::File::create(&partial)?;
                let count = std::io::copy(&mut input, &mut output)?;
                output.flush()?;
                drop(output);
                if count != self.size || sha1_path(&partial)? != self.sha1 {
                    return Err("Mojang file checksum or size mismatch".into());
                }
                replace(&partial, &self.path)?;
                Ok(())
            })();
            if result.is_ok() {
                return result;
            }
            let _ = fs::remove_file(&partial);
            if attempt == 2 {
                return result;
            }
            std::thread::sleep(std::time::Duration::from_millis(500 * (attempt + 1)));
        }
        unreachable!()
    }
}

fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() {
        if to.is_dir() {
            fs::remove_dir_all(to)?;
        } else {
            fs::remove_file(to)?;
        }
    }
    fs::rename(from, to)
}

fn relative(name: &str) -> Result<&Path> {
    let path = Path::new(name);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
        || name.contains('\\')
    {
        return Err("unsafe asset path".into());
    }
    Ok(path)
}

fn extract_engine_data(app: &AndroidApp, stage: &Path) -> Result<()> {
    let mut asset = app
        .asset_manager()
        .open(c"engine-data.zip")
        .ok_or("APK is missing Horizon engine-data.zip")?;
    let mut bytes = Vec::new();
    asset.read_to_end(&mut bytes)?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.enclosed_name().ok_or("unsafe engine-data path")?;
        if entry.is_symlink() {
            return Err("engine data contains a symlink".into());
        }
        let path = stage.join(name);
        fs::create_dir_all(path.parent().ok_or("missing engine-data parent")?)?;
        std::io::copy(&mut entry, &mut fs::File::create(path)?)?;
    }
    Ok(())
}

pub fn ensure(home: &Path, app: &AndroidApp) -> Result<PathBuf> {
    if ready(home) {
        return Ok(resource_root(home));
    }

    let version = horizon_core::VANILLA_VERSION;
    let pins: Value = serde_json::from_str(LOCK)?;
    let pins = &pins["minecraft"];
    if pins["version"] != version {
        return Err("research lock and Horizon version disagree".into());
    }

    log::info!("First run: preparing Minecraft {version} directly from Mojang");
    let catalog = json(CATALOG)?;
    let entry = catalog["versions"]
        .as_array()
        .and_then(|versions| versions.iter().find(|v| v["id"] == version))
        .ok_or("pinned Minecraft version is not listed by Mojang")?;
    let metadata_sha1 = entry["sha1"]
        .as_str()
        .ok_or("missing Mojang version metadata hash")?;
    let accepted = pins["manifest_sha1"] == metadata_sha1
        || pins["accepted_manifest_sha1"]
            .as_array()
            .is_some_and(|values| values.iter().any(|value| value == metadata_sha1));
    if !accepted {
        return Err("Mojang changed this version metadata; Horizon update required".into());
    }

    let manifest_url = entry["url"].as_str().ok_or("missing version metadata URL")?;
    let manifest_bytes = get_bytes(manifest_url, MAX_METADATA)?;
    if format!("{:x}", Sha1::digest(&manifest_bytes)) != metadata_sha1 {
        return Err("version metadata checksum mismatch".into());
    }
    let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
    if manifest["id"] != version
        || manifest["downloads"]["client"]["sha1"] != pins["client_sha1"]
    {
        return Err("Minecraft client differs from Horizon's pinned version".into());
    }

    let cache = home.join(".local/downloads").join(version);
    fs::create_dir_all(&cache)?;
    let index_download = Download::from(&manifest["assetIndex"], cache.join("asset-index.json"))?;
    index_download.get()?;
    let index: Value = serde_json::from_reader(fs::File::open(&index_download.path)?)?;
    let objects = index["objects"].as_object().ok_or("missing Mojang asset objects")?;
    let client = Download::from(&manifest["downloads"]["client"], cache.join("client.jar"))?;

    let mut files = vec![client.clone()];
    let mut assets = Vec::with_capacity(objects.len());
    let mut seen = HashSet::new();
    for (name, object) in objects {
        #[cfg(not(feature = "audio"))]
        if name.starts_with("minecraft/sounds/") || name == "minecraft/sounds.json" {
            continue;
        }
        relative(name)?;
        let checksum = object["hash"].as_str().ok_or("missing asset hash")?;
        if checksum.len() != 40 || !checksum.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("invalid Mojang asset hash".into());
        }
        let path = cache.join("objects").join(checksum);
        assets.push((name.clone(), path.clone()));
        if seen.insert(checksum.to_owned()) {
            files.push(Download::from(
                &serde_json::json!({
                    "url": format!("https://resources.download.minecraft.net/{}/{checksum}", &checksum[..2]),
                    "sha1": checksum,
                    "size": object["size"]
                }),
                path,
            )?);
        }
    }

    let total: u64 = files.iter().map(|file| file.size).sum();
    log::info!(
        "Minecraft {version}: {} verified Mojang downloads ({} MiB)",
        files.len(),
        total / 1_048_576
    );
    let next = AtomicUsize::new(0);
    let done = AtomicU64::new(0);
    let error = Mutex::new(None::<String>);
    std::thread::scope(|scope| {
        for _ in 0..6 {
            scope.spawn(|| loop {
                if error.lock().expect("download error mutex").is_some() {
                    break;
                }
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(file) = files.get(index) else {
                    break;
                };
                if let Err(e) = file.get() {
                    *error.lock().expect("download error mutex") = Some(e.to_string());
                    break;
                }
                let now = done.fetch_add(file.size, Ordering::Relaxed) + file.size;
                log::info!("Mojang resources: {}%", now.saturating_mul(100) / total.max(1));
            });
        }
    });
    if let Some(error) = error.into_inner().expect("download error mutex") {
        return Err(error.into());
    }

    let versions = home.join(".local/horizon-client");
    fs::create_dir_all(&versions)?;
    let stage = versions.join(format!("{version}.partial"));
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    fs::create_dir_all(&stage)?;

    let install = (|| -> Result<()> {
        let mut client_jar = zip::ZipArchive::new(fs::File::open(&client.path)?)?;
        for index in 0..client_jar.len() {
            let mut entry = client_jar.by_index(index)?;
            if entry.is_dir()
                || !(entry.name().starts_with("assets/") || entry.name().starts_with("data/"))
            {
                continue;
            }
            if entry.is_symlink() {
                return Err("symlink in Minecraft client resources".into());
            }
            let name = entry.enclosed_name().ok_or("unsafe client resource path")?;
            let path = stage.join(name);
            fs::create_dir_all(path.parent().ok_or("missing client-resource parent")?)?;
            std::io::copy(&mut entry, &mut fs::File::create(path)?)?;
        }
        for (name, from) in &assets {
            let path = stage.join("assets").join(relative(name)?);
            fs::create_dir_all(path.parent().ok_or("missing asset parent")?)?;
            fs::copy(from, path)?;
        }
        extract_engine_data(app, &stage)?;
        fs::copy(&index_download.path, stage.join("asset-index.json"))?;
        if !stage.join("horizon-resources.json").is_file() {
            return Err("APK engine data is incomplete for this Horizon build".into());
        }
        #[cfg(feature = "audio")]
        fs::write(stage.join("audio-complete.json"), b"{}")?;
        Ok(())
    })();

    if let Err(error) = install {
        let _ = fs::remove_dir_all(&stage);
        return Err(error);
    }

    let destination = resource_root(home);
    let backup = versions.join(format!("{version}.previous"));
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    if destination.exists() {
        fs::rename(&destination, &backup)?;
    }
    if let Err(error) = fs::rename(&stage, &destination) {
        if backup.exists() {
            let _ = fs::rename(&backup, &destination);
        }
        return Err(error.into());
    }
    if backup.exists() {
        let _ = fs::remove_dir_all(backup);
    }
    log::info!("Minecraft {version} resources installed from Mojang");
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_mojang_https_is_accepted() {
        assert!(official_url("https://piston-data.mojang.com/objects/game.jar").is_ok());
        for url in [
            "http://piston-data.mojang.com/game.jar",
            "https://piston-data.mojang.com.evil.test/game.jar",
            "https://piston-data.mojang.com@evil.test/game.jar",
            "http://127.0.0.1:8123/game.zip",
        ] {
            assert!(official_url(url).is_err(), "accepted {url}");
        }
    }

    #[test]
    fn asset_paths_cannot_escape_installation() {
        assert!(relative("minecraft/lang/en_us.json").is_ok());
        for name in [
            "../escape",
            "/absolute",
            "minecraft/../escape",
            "minecraft\\..\\escape",
        ] {
            assert!(relative(name).is_err(), "accepted {name}");
        }
    }
}

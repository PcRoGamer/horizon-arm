//! Native resource installation from Mojang HTTPS endpoints. No Java or LAN server.
use crate::Result;
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

const CATALOG: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const LOCK: &str = include_str!("../../../research-lock.json");

/// Earlier Android packages installed textures only. An audio-capable APK must
/// upgrade those resources before entering the game, not silently stay mute.
pub fn ready(home: &Path) -> bool {
    let root = home
        .join(".local/horizon-client")
        .join(horizon_core::VANILLA_VERSION);
    crate::resource_install::ready(home)
        && (!cfg!(feature = "audio") || root.join("audio-complete.json").is_file())
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
        return Err("Download URL is not an official Mojang HTTPS endpoint".into());
    }
    Ok(())
}

fn agent() -> ureq::Agent {
    // Thousands of sound objects share a host; retain pooled TLS connections.
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

fn json(url: &str) -> Result<Value> {
    official_url(url)?;
    let mut bytes = Vec::new();
    agent()
        .get(url)
        .call()?
        .into_reader()
        .take(16 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha1::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

struct Download {
    url: String,
    sha1: String,
    size: u64,
    path: PathBuf,
}

impl Download {
    fn from(value: &Value, path: PathBuf) -> Result<Self> {
        let file = Self {
            url: value["url"].as_str().ok_or("Missing download URL")?.into(),
            sha1: value["sha1"]
                .as_str()
                .ok_or("Missing download hash")?
                .into(),
            size: value["size"].as_u64().ok_or("Missing download size")?,
            path,
        };
        official_url(&file.url)?;
        if file.sha1.len() != 40
            || !file.sha1.bytes().all(|c| c.is_ascii_hexdigit())
            || file.size > 1024 * 1024 * 1024
        {
            return Err("Invalid Mojang download metadata".into());
        }
        Ok(file)
    }
    fn get(&self) -> Result<()> {
        if self.path.is_file()
            && fs::metadata(&self.path)?.len() == self.size
            && hash(&self.path)? == self.sha1
        {
            return Ok(());
        }
        fs::create_dir_all(self.path.parent().ok_or("Missing download directory")?)?;
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
                if count != self.size || hash(&partial)? != self.sha1 {
                    return Err("Mojang file checksum or size mismatch".into());
                }
                fs::rename(&partial, &self.path)?;
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

fn relative(name: &str) -> Result<&Path> {
    let path = Path::new(name);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
        || name.contains('\\')
    {
        return Err("Unsafe asset path".into());
    }
    Ok(path)
}

pub fn install(home: &Path, progress: impl Fn(u64, u64) + Sync) -> Result<()> {
    let version = horizon_core::VANILLA_VERSION;
    let pins: Value = serde_json::from_str(LOCK)?;
    let pins = &pins["minecraft"];
    let catalog = json(CATALOG)?;
    let entry = catalog["versions"]
        .as_array()
        .and_then(|vs| vs.iter().find(|v| v["id"] == version))
        .ok_or("Pinned Minecraft version is not listed by Mojang")?;
    let checksum = entry["sha1"]
        .as_str()
        .ok_or("Missing version metadata hash")?;
    if pins["manifest_sha1"] != checksum
        && !pins["accepted_manifest_sha1"]
            .as_array()
            .is_some_and(|v| v.iter().any(|x| x == checksum))
    {
        return Err("Mojang changed this version; engine update required".into());
    }
    let cache = home.join(".local/downloads").join(version);
    fs::create_dir_all(&cache)?;
    let manifest_url = entry["url"]
        .as_str()
        .ok_or("Missing version metadata URL")?;
    official_url(manifest_url)?;
    let mut bytes = Vec::new();
    agent()
        .get(manifest_url)
        .call()?
        .into_reader()
        .take(16 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    if format!("{:x}", Sha1::digest(&bytes)) != checksum {
        return Err("Version metadata checksum mismatch".into());
    }
    let manifest: Value = serde_json::from_slice(&bytes)?;
    if manifest["id"] != version || manifest["downloads"]["client"]["sha1"] != pins["client_sha1"] {
        return Err("Minecraft client differs from this engine's pin".into());
    }
    let index = Download::from(&manifest["assetIndex"], cache.join("asset-index.json"))?;
    index.get()?;
    let index: Value = serde_json::from_reader(fs::File::open(&index.path)?)?;
    let objects = index["objects"]
        .as_object()
        .ok_or("Missing asset objects")?;
    let client = Download::from(&manifest["downloads"]["client"], cache.join("client.jar"))?;
    let mut files = vec![client];
    let mut assets = Vec::new();
    let mut seen = HashSet::new();
    for (name, object) in objects {
        if !cfg!(feature = "audio")
            && (name.starts_with("minecraft/sounds/") || name == "minecraft/sounds.json")
        {
            continue;
        }
        relative(name)?;
        let checksum = object["hash"].as_str().ok_or("Missing asset hash")?;
        if checksum.len() != 40 || !checksum.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid asset hash".into());
        }
        let path = cache.join("objects").join(checksum);
        assets.push((name, path.clone()));
        if seen.insert(checksum) {
            files.push(Download::from(&serde_json::json!({"url":format!("https://resources.download.minecraft.net/{}/{checksum}", &checksum[..2]), "sha1":checksum, "size":object["size"]}), path)?);
        }
    }
    let total: u64 = files.iter().map(|f| f.size).sum();
    log::info!(
        "Minecraft {version}: downloading {} files directly from Mojang ({} MiB)",
        files.len(),
        total / 1048576
    );
    progress(0, total);
    let next = AtomicUsize::new(0);
    let done = AtomicU64::new(0);
    let error = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                loop {
                    if error.lock().unwrap().is_some() {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(file) = files.get(i) else { break };
                    if let Err(e) = file.get() {
                        *error.lock().unwrap() = Some(e.to_string());
                        break;
                    }
                    progress(
                        done.fetch_add(file.size, Ordering::Relaxed) + file.size,
                        total,
                    );
                }
            });
        }
    });
    if let Some(error) = error.into_inner().unwrap() {
        return Err(error.into());
    }
    let versions = home.join(".local/horizon-client");
    fs::create_dir_all(&versions)?;
    let stage = versions.join("official.partial");
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    fs::create_dir(&stage)?;
    let result = (|| -> Result<()> {
        let mut zip = zip::ZipArchive::new(fs::File::open(&files[0].path)?)?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if !(entry.name().starts_with("assets/") || entry.name().starts_with("data/"))
                || entry.is_dir()
            {
                continue;
            }
            let name = entry.enclosed_name().ok_or("Unsafe client resource path")?;
            if entry.is_symlink() {
                return Err("Symlink in client resources".into());
            }
            let path = stage.join(name);
            fs::create_dir_all(path.parent().ok_or("Missing resource parent")?)?;
            std::io::copy(&mut entry, &mut fs::File::create(path)?)?;
        }
        for (name, from) in assets {
            let path = stage.join("assets").join(relative(name)?);
            fs::create_dir_all(path.parent().ok_or("Missing asset parent")?)?;
            fs::copy(from, path)?;
        }
        crate::resource_install::extract(&home.join("engine-data.zip"), &stage)?;
        fs::copy(
            cache.join("asset-index.json"),
            stage.join("asset-index.json"),
        )?;
        if !stage.join("horizon-resources.json").is_file() {
            return Err("APK has no engine data for this version".into());
        }
        if cfg!(feature = "audio") {
            fs::write(stage.join("audio-complete.json"), b"{}")?;
        }
        let destination = versions.join(version);
        let backup = versions.join(format!("{version}.previous"));
        if backup.exists() && !destination.exists() {
            fs::rename(&backup, &destination)?;
        }
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
            let _ = fs::remove_dir_all(&backup);
        }
        Ok(())
    })();
    if stage.exists() {
        let _ = fs::remove_dir_all(stage);
    }
    result
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
            assert!(official_url(url).is_err());
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
            assert!(relative(name).is_err());
        }
    }
}

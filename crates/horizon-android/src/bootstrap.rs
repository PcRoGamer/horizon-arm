//! First-run Android bootstrap.
//!
//! The APK carries only Horizon-computed, version-specific tables. Minecraft's
//! client and server jars are downloaded directly from Mojang on the phone,
//! checked against the pins in `research-lock.json`, and combined with those
//! tables into the normal Horizon resource folder. No Microsoft login or Java
//! runtime is involved in this path.

use std::{
    collections::BTreeMap,
    ffi::CString,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use android_activity::AndroidApp;
use serde_json::{Map, Value};
use sha1::Digest as _;

use crate::bundled::Progress;

const VERSION: &str = "26.3";
const FACTS_ASSET: &str = "horizon-facts-26.3.zip";
const MANIFEST_SHA1: &str = "11f1aefe473eefba0af62e3286c33b692807b2eb";
const CLIENT_SHA1: &str = "e877b6a07acd633fb3bb475002175cec036e7b87";
const SERVER_SHA1: &str = "33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c";

const WORLDGEN_BUNDLES: [(&str, &str); 19] = [
    ("noises", "worldgen/noise"),
    ("densities", "worldgen/density_function"),
    ("rules", "worldgen/material_rule"),
    ("conditions", "worldgen/material_condition"),
    ("settings", "worldgen/noise_settings"),
    ("carvers", "worldgen/carver"),
    ("biomes", "worldgen/biome"),
    ("world_presets", "worldgen/world_preset"),
    ("features", "worldgen/feature"),
    ("placed_features", "worldgen/placed_feature"),
    ("state_providers", "worldgen/block_state_provider"),
    ("biome_tags", "tags/worldgen/biome"),
    ("feature_tags", "tags/worldgen/feature"),
    ("block_tags", "tags/block"),
    ("fluid_tags", "tags/fluid"),
    ("processor_lists", "worldgen/processor_list"),
    ("template_pools", "worldgen/template_pool"),
    ("structures", "worldgen/structure"),
    ("structure_sets", "worldgen/structure_set"),
];

const PRECOMPUTED_BLOCK_FILES: [&str; 7] = [
    "block-properties.nbt",
    "cached-collision.nbt",
    "cached-support.nbt",
    "survival-rules.json",
    "survival-extra-definitions.json",
    "placement-definitions.json",
    "structure-blocks.json",
];

pub(super) fn prepared(home: &Path) -> bool {
    home.join(".local/horizon-client")
        .join(VERSION)
        .join("horizon-resources.json")
        .is_file()
}

/// Download the two Mojang jars and assemble a complete resource directory.
pub(super) fn run(app: &AndroidApp, home: &Path, progress: Arc<Progress>) -> Result<PathBuf, String> {
    progress.total.store(100, Ordering::Relaxed);
    set(&progress, 1);

    let work = home.join(".horizon-bootstrap").join(VERSION);
    fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;

    let facts_zip = work.join("facts.zip");
    copy_asset(app, FACTS_ASSET, &facts_zip, &progress)?;
    set(&progress, 8);

    let destination = home.join(".local/horizon-client").join(VERSION);
    let staging = destination
        .parent()
        .unwrap_or(home)
        .join(format!(".{VERSION}.assembling"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
    unpack_facts(&facts_zip, &staging, &progress)?;
    set(&progress, 20);

    let facts_manifest = read_json(&staging.join("horizon-resources.json"))?;
    if facts_manifest.get("minecraft").and_then(Value::as_str) != Some(VERSION) {
        return Err("the bundled Horizon facts are not for Minecraft 26.3".into());
    }

    let manifest_url = format!(
        "https://piston-meta.mojang.com/v1/packages/{MANIFEST_SHA1}/{VERSION}.json"
    );
    let manifest_bytes = get_bytes(&manifest_url)?;
    if sha1_hex(&manifest_bytes) != MANIFEST_SHA1 {
        return Err("Mojang's 26.3 version manifest did not match Horizon's pin".into());
    }
    let manifest: Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| format!("Mojang's 26.3 manifest: {e}"))?;
    if manifest.get("id").and_then(Value::as_str) != Some(VERSION) {
        return Err("Mojang's pinned manifest does not describe 26.3".into());
    }
    let client = download_info(&manifest, "client", CLIENT_SHA1)?;
    let server = download_info(&manifest, "server", SERVER_SHA1)?;
    set(&progress, 24);

    let client_jar = work.join("client.jar");
    download_checked(&client.url, &client_jar, &client.sha1, client.size, &progress)?;
    set(&progress, 34);
    let server_bundle = work.join("server.jar");
    download_checked(&server.url, &server_bundle, &server.sha1, server.size, &progress)?;
    set(&progress, 44);

    let expected_client = text_field(&facts_manifest, "client_sha256")?;
    if sha256_file(&client_jar)? != expected_client {
        return Err("the downloaded client jar differs from the one used to compute Horizon's facts".into());
    }

    let server_game = work.join("server-game.jar");
    extract_server_game(&server_bundle, &server_game, &facts_manifest)?;
    set(&progress, 48);

    let (data_version, resource_files) = extract_client_resources(&client_jar, &staging, &progress)?;
    set(&progress, 80);
    write_block_tags(&server_game, &client_jar, &staging)?;
    write_worldgen(&server_game, &client_jar, &staging, &progress)?;

    // Keep the precomputed manifest's exporter/source hashes, but refresh the two
    // fields that describe data copied from the player's Mojang jars.
    let mut final_manifest = facts_manifest;
    if let Some(object) = final_manifest.as_object_mut() {
        object.insert("data_version".into(), data_version);
        object.insert("resource_files".into(), Value::from(resource_files as u64));
    }
    fs::write(
        staging.join("horizon-resources.json"),
        serde_json::to_vec_pretty(&final_manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    for name in PRECOMPUTED_BLOCK_FILES {
        let path = staging.join("worldgen/blocks").join(name);
        if !path.is_file() {
            return Err(format!("the Horizon facts pack is missing worldgen/blocks/{name}"));
        }
    }

    set(&progress, 99);
    fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&destination);
    fs::rename(&staging, &destination)
        .map_err(|e| format!("{} -> {}: {e}", staging.display(), destination.display()))?;

    // The assembled resources contain everything Horizon reads at runtime.
    // Removing the jars also means an ordinary app update does not redownload
    // anything while this resource set remains current.
    let _ = fs::remove_file(&client_jar);
    let _ = fs::remove_file(&server_bundle);
    let _ = fs::remove_file(&server_game);
    set(&progress, 100);
    Ok(destination)
}

struct Download {
    url: String,
    sha1: String,
    size: Option<u64>,
}

fn download_info(manifest: &Value, side: &str, pinned: &str) -> Result<Download, String> {
    let item = manifest
        .get("downloads")
        .and_then(|v| v.get(side))
        .ok_or_else(|| format!("Mojang's manifest has no {side} download"))?;
    let sha1 = item
        .get("sha1")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Mojang's {side} download has no sha1"))?;
    if sha1 != pinned {
        return Err(format!("Mojang's {side} sha1 differs from Horizon's 26.3 pin"));
    }
    Ok(Download {
        url: item
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Mojang's {side} download has no URL"))?
            .to_string(),
        sha1: sha1.to_string(),
        size: item.get("size").and_then(Value::as_u64),
    })
}

fn copy_asset(app: &AndroidApp, name: &str, to: &Path, progress: &Progress) -> Result<(), String> {
    let manager = app.asset_manager();
    let mut asset = manager
        .open(&CString::new(name).map_err(|e| e.to_string())?)
        .ok_or_else(|| format!("APK is missing {name}"))?;
    let mut out = File::create(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let copied = std::io::copy(&mut asset, &mut out).map_err(|e| e.to_string())?;
    progress.bytes.fetch_add(copied, Ordering::Relaxed);
    Ok(())
}

fn unpack_facts(zip_path: &Path, out: &Path, progress: &Progress) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let total = archive.len().max(1);
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let relative = entry
            .enclosed_name()
            .ok_or("unsafe path in Horizon facts archive")?
            .to_path_buf();
        let path = out.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut target = File::create(&path).map_err(|e| e.to_string())?;
            let bytes = std::io::copy(&mut entry, &mut target).map_err(|e| e.to_string())?;
            progress.bytes.fetch_add(bytes, Ordering::Relaxed);
        }
        set(progress, 8 + ((i + 1) * 12 / total));
    }
    Ok(())
}

fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let response = ureq::AgentBuilder::new()
        .user_agent("horizon-android/0.1")
        .build()
        .get(url)
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let mut data = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut data)
        .map_err(|e| format!("{url}: {e}"))?;
    Ok(data)
}

fn download_checked(
    url: &str,
    path: &Path,
    expected_sha1: &str,
    expected_size: Option<u64>,
    progress: &Progress,
) -> Result<(), String> {
    if path.is_file()
        && expected_size.is_none_or(|size| fs::metadata(path).is_ok_and(|m| m.len() == size))
        && sha1_file(path).is_ok_and(|hash| hash == expected_sha1)
    {
        return Ok(());
    }
    let temp = path.with_extension("part");
    let response = ureq::AgentBuilder::new()
        .user_agent("horizon-android/0.1")
        .build()
        .get(url)
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let mut input = response.into_reader();
    let mut output = File::create(&temp).map_err(|e| e.to_string())?;
    let mut sha = sha1::Sha1::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let n = input.read(&mut buffer).map_err(|e| format!("{url}: {e}"))?;
        if n == 0 {
            break;
        }
        output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        sha.update(&buffer[..n]);
        size += n as u64;
        progress.bytes.fetch_add(n as u64, Ordering::Relaxed);
    }
    output.flush().map_err(|e| e.to_string())?;
    if expected_size.is_some_and(|expected| expected != size) {
        let _ = fs::remove_file(&temp);
        return Err(format!("{url}: downloaded {size} bytes, expected {}", expected_size.unwrap()));
    }
    let actual = hex(&sha.finalize());
    if actual != expected_sha1 {
        let _ = fs::remove_file(&temp);
        return Err(format!("{url}: sha1 {actual}, expected {expected_sha1}"));
    }
    fs::rename(&temp, path).map_err(|e| e.to_string())
}

fn extract_server_game(bundle: &Path, output: &Path, facts: &Value) -> Result<(), String> {
    let file = File::open(bundle).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut list = String::new();
    archive
        .by_name("META-INF/versions.list")
        .map_err(|e| format!("server bundle versions.list: {e}"))?
        .read_to_string(&mut list)
        .map_err(|e| e.to_string())?;
    let rows: Vec<&str> = list.lines().filter(|line| !line.trim().is_empty()).collect();
    if rows.len() != 1 {
        return Err("unexpected Minecraft server bundle version list".into());
    }
    let fields: Vec<&str> = rows[0].split('\t').collect();
    if fields.len() != 3 || fields[1] != VERSION || !safe_relative(fields[2]) {
        return Err("unexpected Minecraft server bundle entry".into());
    }
    let member = format!("META-INF/versions/{}", fields[2]);
    let mut source = archive.by_name(&member).map_err(|e| format!("{member}: {e}"))?;
    let mut out = File::create(output).map_err(|e| e.to_string())?;
    std::io::copy(&mut source, &mut out).map_err(|e| e.to_string())?;
    let actual = sha256_file(output)?;
    if actual != fields[0] || actual != text_field(facts, "server_sha256")? {
        return Err("the server game jar differs from the one used to compute Horizon's facts".into());
    }
    Ok(())
}

fn extract_client_resources(
    client_path: &Path,
    out: &Path,
    progress: &Progress,
) -> Result<(Value, usize), String> {
    let file = File::open(client_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let version: Value = {
        let mut text = String::new();
        archive
            .by_name("version.json")
            .map_err(|e| format!("client version.json: {e}"))?
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())?
    };
    let data_version = version
        .get("world_version")
        .cloned()
        .ok_or("client version.json has no world_version")?;

    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.name_for_index(i).map(str::to_string))
        .filter(|name| {
            (name.starts_with("assets/") || name.starts_with("data/") || name == "pack.png")
                && !name.ends_with('/')
        })
        .collect();
    let total = names.len().max(1);
    for (i, name) in names.iter().enumerate() {
        let relative = checked_member(name)?;
        let path = out.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut source = archive.by_name(name).map_err(|e| e.to_string())?;
        let mut target = File::create(&path).map_err(|e| e.to_string())?;
        let copied = std::io::copy(&mut source, &mut target).map_err(|e| e.to_string())?;
        progress.bytes.fetch_add(copied, Ordering::Relaxed);
        set(progress, 48 + ((i + 1) * 32 / total));
    }
    Ok((data_version, names.len()))
}

fn write_block_tags(server: &Path, client: &Path, out: &Path) -> Result<(), String> {
    let server = json_resources(server, "data/minecraft/tags/block/")?;
    let client = json_resources(client, "data/minecraft/tags/block/")?;
    if server != client {
        return Err("client and server block tags disagree".into());
    }
    write_json_value(&out.join("block-tags.json"), &Value::Object(server))
}

fn write_worldgen(server: &Path, client: &Path, out: &Path, progress: &Progress) -> Result<(), String> {
    let worldgen = out.join("worldgen");
    fs::create_dir_all(&worldgen).map_err(|e| e.to_string())?;

    let file = File::open(server).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let prefix = "data/minecraft/structure/";
    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.name_for_index(i).map(str::to_string))
        .filter(|name| name.starts_with(prefix) && name.ends_with(".nbt"))
        .collect();
    for name in names {
        let relative = checked_member(name.strip_prefix(prefix).unwrap())?;
        let path = worldgen.join("structure/minecraft").join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut source = archive.by_name(&name).map_err(|e| e.to_string())?;
        let mut target = File::create(path).map_err(|e| e.to_string())?;
        std::io::copy(&mut source, &mut target).map_err(|e| e.to_string())?;
    }

    for (i, (name, path)) in WORLDGEN_BUNDLES.iter().enumerate() {
        let prefix = format!("data/minecraft/{path}/");
        let a = json_resources(server, &prefix)?;
        let b = json_resources(client, &prefix)?;
        if a.is_empty() || a != b {
            return Err(format!("client and server worldgen resources disagree: {path}"));
        }
        write_json_value(&worldgen.join(format!("{name}.json")), &Value::Object(a))?;
        set(progress, 82 + ((i + 1) * 10 / WORLDGEN_BUNDLES.len()));
    }

    let facts = read_json(&out.join("facts.json"))?;
    let states = facts
        .get("states")
        .and_then(Value::as_array)
        .ok_or("facts.json has no states")?;
    let mut flags = Vec::with_capacity(states.len());
    for state in states {
        let row = state.as_array().ok_or("a block state in facts.json is not an array")?;
        if row.len() < 3 {
            return Err("a block state in facts.json is short".into());
        }
        flags.push(Value::Array(vec![row[1].clone(), row[2].clone()]));
    }
    write_json_value(&worldgen.join("block_flags.json"), &Value::Array(flags))?;

    let biomes = out.join("biome-parameters");
    if biomes.join("overworld.json").is_file() {
        copy_tree(&biomes, &worldgen.join("biome_parameters"))?;
    } else if !worldgen.join("presets.json").is_file() {
        return Err("Horizon facts contain no biome parameter tables".into());
    }
    set(progress, 98);
    Ok(())
}

fn json_resources(jar: &Path, prefix: &str) -> Result<Map<String, Value>, String> {
    let file = File::open(jar).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.name_for_index(i).map(str::to_string))
        .filter(|name| name.starts_with(prefix) && name.ends_with(".json"))
        .collect();
    names.sort();
    let mut map = Map::new();
    for name in names {
        let mut text = String::new();
        archive
            .by_name(&name)
            .map_err(|e| e.to_string())?
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        let key = format!("minecraft:{}", &name[prefix.len()..name.len() - 5]);
        map.insert(key, Value::String(text));
    }
    Ok(map)
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_tree(&source, &target)?;
        } else {
            fs::copy(&source, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn checked_member(name: &str) -> Result<PathBuf, String> {
    let path = Path::new(name);
    if path.is_absolute()
        || name.starts_with('/')
        || path.components().any(|component| {
            !matches!(component, Component::Normal(_) | Component::CurDir)
        })
    {
        return Err(format!("unsafe path in Minecraft jar: {name}"));
    }
    Ok(path.to_path_buf())
}

fn safe_relative(name: &str) -> bool {
    checked_member(name).is_ok() && !name.contains('\\')
}

fn read_json(path: &Path) -> Result<Value, String> {
    let data = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&data).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_json_value(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn text_field<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Horizon facts manifest has no {key}"))
}

fn sha1_hex(data: &[u8]) -> String {
    hex(&sha1::Sha1::digest(data))
}

fn sha1_file(path: &Path) -> Result<String, String> {
    hash_file::<sha1::Sha1>(path)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    hash_file::<sha2::Sha256>(path)
}

fn hash_file<D: Digest + Default>(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = D::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex(&hash.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn set(progress: &Progress, value: usize) {
    progress.files.store(value.min(100), Ordering::Relaxed);
}

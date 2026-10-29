//! Android's bundled first-party Horizon shader pack.
//!
//! Golden Hour remains a normal Horizon shader pack on disk so the existing
//! shader-pack scanner, option page and hot-reload path all see the same thing
//! they do on desktop. The APK merely guarantees that the official example is
//! available in every instance without a separate download.

use std::{fs, path::Path};

const PACK: &str = "golden_hour";
const MIGRATION: &str = ".horizon-android-golden-hour-default-v1";
const GRAPHICS: &str = "horizon-graphics.txt";

const FILES: &[(&str, &str)] = &[
    (
        "horizon-shaders.toml",
        include_str!("../../horizon-shader-api/packs/golden_hour/horizon-shaders.toml"),
    ),
    (
        "common.wgsl",
        include_str!("../../horizon-shader-api/packs/golden_hour/common.wgsl"),
    ),
    (
        "sky.wgsl",
        include_str!("../../horizon-shader-api/packs/golden_hour/sky.wgsl"),
    ),
    (
        "post/glow.wgsl",
        include_str!("../../horizon-shader-api/packs/golden_hour/post/glow.wgsl"),
    ),
    (
        "post/grade.wgsl",
        include_str!("../../horizon-shader-api/packs/golden_hour/post/grade.wgsl"),
    ),
];

/// Make Horizon's official Golden Hour pack available to this instance and,
/// exactly once, choose it when the player has not selected another pack.
///
/// Existing pack files are never overwritten: a player can edit the bundled
/// example and keep those edits. The migration marker means turning shaders off
/// later is respected instead of Golden Hour being forced on every launch.
pub fn ensure_golden_hour(home: &Path) -> Result<(), String> {
    let pack = home.join("shaderpacks").join(PACK);
    fs::create_dir_all(pack.join("post"))
        .map_err(|e| format!("create {}: {e}", pack.display()))?;

    for (relative, contents) in FILES {
        let path = pack.join(relative);
        if path.exists() {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        fs::write(&path, contents).map_err(|e| format!("write {}: {e}", path.display()))?;
    }

    let marker = home.join(MIGRATION);
    if marker.exists() {
        return Ok(());
    }

    let graphics = home.join(GRAPHICS);
    let current = fs::read_to_string(&graphics).unwrap_or_default();
    let mut found = false;
    let mut has_pick = false;
    let mut lines = Vec::new();
    for line in current.lines() {
        let Some((key, value)) = line.split_once(':') else {
            lines.push(line.to_owned());
            continue;
        };
        if key.trim() == "pickedShaderPack" {
            found = true;
            if value.trim().is_empty() {
                lines.push(format!("pickedShaderPack:{PACK}"));
            } else {
                has_pick = true;
                lines.push(line.to_owned());
            }
        } else {
            lines.push(line.to_owned());
        }
    }
    if !found {
        lines.push(format!("pickedShaderPack:{PACK}"));
    }

    if !has_pick {
        let mut text = lines.join("\n");
        text.push('\n');
        fs::write(&graphics, text)
            .map_err(|e| format!("write {}: {e}", graphics.display()))?;
    }

    fs::write(&marker, b"Golden Hour 1.0.0\n")
        .map_err(|e| format!("write {}: {e}", marker.display()))?;
    Ok(())
}

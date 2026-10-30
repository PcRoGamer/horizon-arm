//! Android's bundled first-party Horizon shader pack.
//!
//! Golden Hour remains available as a normal Horizon shader pack on disk, but
//! it is no longer forced on by the Android build. The first Android shader
//! build briefly made it the default; the v2 migration removes only that old
//! automatic choice and leaves the pack installed for anyone who wants it.

use std::{fs, path::Path};

const PACK: &str = "golden_hour";
const OLD_DEFAULT_MIGRATION: &str = ".horizon-android-golden-hour-default-v1";
const DEFAULT_REMOVED_MIGRATION: &str = ".horizon-android-golden-hour-default-removed-v2";
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

/// Make Horizon's first-party Golden Hour example available to this instance.
/// It is deliberately *not* selected by default.
///
/// Existing pack files are never overwritten. If this install previously got
/// Golden Hour only because Android v1 auto-selected it, clear that one old
/// default exactly once; the player can still choose Golden Hour again later.
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

    let migrated = home.join(DEFAULT_REMOVED_MIGRATION);
    if migrated.exists() {
        return Ok(());
    }

    // Only installations touched by the short-lived v1 auto-default are
    // eligible for cleanup. Fresh installs simply start with shaders off.
    if home.join(OLD_DEFAULT_MIGRATION).exists() {
        let graphics = home.join(GRAPHICS);
        if let Ok(current) = fs::read_to_string(&graphics) {
            let mut changed = false;
            let mut lines = Vec::new();
            for line in current.lines() {
                if let Some((key, value)) = line.split_once(':')
                    && key.trim() == "pickedShaderPack"
                    && value.trim() == PACK
                {
                    lines.push("pickedShaderPack:".to_owned());
                    changed = true;
                } else {
                    lines.push(line.to_owned());
                }
            }
            if changed {
                let mut text = lines.join("\n");
                text.push('\n');
                fs::write(&graphics, text)
                    .map_err(|e| format!("write {}: {e}", graphics.display()))?;
            }
        }
    }

    fs::write(
        &migrated,
        b"Golden Hour remains available but is no longer an Android default.\n",
    )
    .map_err(|e| format!("write {}: {e}", migrated.display()))?;
    Ok(())
}

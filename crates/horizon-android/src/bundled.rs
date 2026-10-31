//! The game's data, when it travels inside the APK.
//!
//! An APK can carry assets (`aapt2 -A`, an `assets/` folder in the archive),
//! and the NDK's `AssetManager` reads them with no Java. `AAssetDir` lists a
//! directory's *files* only — subdirectories never come back from it — so the
//! build writes the whole file list next to the data (`assets/resources.list`)
//! and this unpacks that list into the folder `find_resources` looks at first,
//! on the first run.
//!
//! A prepared folder that is already on the device (an `adb push`, or
//! `tools/push-resources.sh`) always wins: this only fills in an install that
//! has nothing.

use std::{
    ffi::CString,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

use ndk::asset::AssetManager;

/// The list `assets/resources.list`, written by `tools/build.sh`: one asset
/// path per line (`resources/26.3/assets/…`), sorted.
const LIST: &str = "resources.list";

/// Where a prepared folder for `version` lives under `home`, relative: the
/// second path `find_resources` tries.
fn folder(version: &str) -> PathBuf {
    Path::new(".local/horizon-client").join(version)
}

/// `horizon-prepare` writes `horizon-resources.json`; a folder that has it is
/// a prepared one, and so is a folder this unpacked on an earlier run (the
/// rename at the end only happens once every file is in place).
fn prepared(dest: &Path) -> bool {
    dest.join("horizon-resources.json").is_file()
}

/// Is there data in the APK that is not on the device yet? Answered before any
/// window exists, so the loading screen is only shown when it is needed.
pub(super) fn pending(manager: &AssetManager, home: &Path, version: &str) -> bool {
    manager.open(&CString::new(LIST).unwrap()).is_some()
        && !prepared(&home.join(folder(version)))
}

/// What an unpack did, for the line `android_main` writes to logcat: the log
/// file is not open yet when this runs.
pub(super) struct Unpacked {
    pub dir: PathBuf,
    pub files: usize,
    pub bytes: u64,
    pub missing: usize,
    pub secs: f32,
}

/// The unpack's state, read by the loading screen while the worker writes it.
#[derive(Default)]
pub(super) struct Progress {
    /// Entries in the list: the denominator of the bar. Zero until read.
    pub total: AtomicUsize,
    pub files: AtomicUsize,
    pub bytes: AtomicU64,
    pub done: AtomicBool,
    pub failed: Mutex<Option<String>>,
}

impl Progress {
    /// 0..=1, or `None` while the list has not been read yet.
    pub(super) fn fraction(&self) -> Option<f32> {
        let total = self.total.load(Ordering::Relaxed);
        (total > 0).then(|| (self.files.load(Ordering::Relaxed) as f32 / total as f32).min(1.0))
    }
}

/// Unpacks the bundled data into `home` unless it is there already. Reports
/// through `progress` for the loading screen. Returns what it did, or `None`
/// when there is nothing to do.
pub(super) fn unpack(
    manager: &AssetManager,
    home: &Path,
    version: &str,
    progress: &Progress,
) -> io::Result<Option<Unpacked>> {
    let Some(mut list) = manager.open(&CString::new(LIST).unwrap()) else {
        return Ok(None);
    };
    let mut text = String::new();
    list.read_to_string(&mut text)?;
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    progress.total.store(lines.len(), Ordering::Relaxed);

    let dest = home.join(folder(version));
    if prepared(&dest) {
        // Pushed from a desktop, or unpacked here on an earlier run.
        return Ok(None);
    }

    // Into a sibling first: an interrupted unpack leaves no half-folder that
    // looks prepared, and the rename at the end is atomic.
    let temp = home
        .join(folder(version))
        .parent()
        .map(|dir| dir.join(format!("{version}.unpacking")))
        .unwrap_or_else(|| home.join("resources.unpacking"));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp)?;

    let prefix = format!("resources/{version}/");
    let started = std::time::Instant::now();
    let (mut files, mut written, mut missing) = (0usize, 0u64, 0usize);
    for line in lines {
        let Some(rel) = line.strip_prefix(&prefix) else {
            // A list from another version: unpack only the one asked for,
            // never 26.2's files into 26.3's folder.
            continue;
        };
        let Some(mut asset) = manager.open(&CString::new(line).map_err(io::Error::other)?) else {
            // The packer drops what it will not carry (hidden files, mostly);
            // one missing file must not cost the whole install.
            missing += 1;
            continue;
        };
        let path = temp.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&path)?;
        written += io::copy(&mut asset, &mut file)?;
        files += 1;
        progress.files.store(files, Ordering::Relaxed);
        progress.bytes.store(written, Ordering::Relaxed);
    }
    if files == 0 {
        let _ = fs::remove_dir_all(&temp);
        return Ok(None);
    }
    if prepared(&dest) {
        // Pushed while this ran: theirs wins.
        let _ = fs::remove_dir_all(&temp);
        return Ok(None);
    }
    let _ = fs::remove_dir_all(&dest);
    fs::rename(&temp, &dest)?;
    Ok(Some(Unpacked {
        dir: dest,
        files,
        bytes: written,
        missing,
        secs: started.elapsed().as_secs_f32(),
    }))
}

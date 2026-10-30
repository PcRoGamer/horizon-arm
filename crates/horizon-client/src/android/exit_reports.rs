//! Previous-process exit diagnostics. Read-only system history; bounded local exports.
use jni::objects::{JByteArray, JObject, JString, JValue};
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};
fn tail(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    let n = f.metadata()?.len();
    f.seek(SeekFrom::Start(n.saturating_sub(256 * 1024)))?;
    let mut out = Vec::new();
    f.take(256 * 1024).read_to_end(&mut out)?;
    Ok(out)
}
pub fn collect(home: &Path) {
    let dir = home.join("performance");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    // Preserve previous-run information before log rotation/startup removes it.
    for (source, name) in [
        (home.join("logs/latest.log"), "previous-run.log"),
        (home.join("android-error.txt"), "previous-error.txt"),
    ] {
        if let Ok(data) = tail(&source) {
            let _ = std::fs::write(dir.join(name), data);
        }
    }
    let mut reports = Vec::new();
    crate::platform::with_context(|env, activity| {
        let name = JObject::from(env.new_string("activity")?);
        let manager = env
            .call_method(
                activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&name)],
            )?
            .l()?;
        let null = JObject::null();
        let exits = env
            .call_method(
                manager,
                "getHistoricalProcessExitReasons",
                "(Ljava/lang/String;II)Ljava/util/List;",
                &[JValue::Object(&null), JValue::Int(0), JValue::Int(8)],
            )?
            .l()?;
        let count = env
            .call_method(&exits, "size", "()I", &[])?
            .i()?
            .clamp(0, 8);
        for i in 0..count {
            let row=env.with_local_frame(32,|env|->jni::errors::Result<serde_json::Value>{
   let item=env.call_method(&exits,"get","(I)Ljava/lang/Object;",&[JValue::Int(i)])?.l()?;
   let stamp=env.call_method(&item,"getTimestamp","()J",&[])?.j()?;
   let reason=env.call_method(&item,"getReason","()I",&[])?.i()?;
   let status=env.call_method(&item,"getStatus","()I",&[])?.i()?;
   let desc=env.call_method(&item,"getDescription","()Ljava/lang/String;",&[])?.l()?;
   let description=if desc.is_null(){String::new()}else{env.get_string(&JString::from(desc))?.to_string_lossy().chars().take(4096).collect()};
   // Native-crash and ANR traces are available on supported firmware. No broad
   // logcat access or external-storage permission is requested.
   let mut trace_file=None;
   if matches!(reason,4..=6) {
    let stream=env.call_method(&item,"getTraceInputStream","()Ljava/io/InputStream;",&[])?.l()?;
    if !stream.is_null(){let buf=env.new_byte_array(8192)?;let mut data=Vec::new();
     let read=(||->jni::errors::Result<()>{while data.len()<256*1024 {let n=env.call_method(&stream,"read","([B)I",&[JValue::Object(&buf)])?.i()?;if n<=0{break;}let bytes=env.convert_byte_array(JByteArray::from(env.new_local_ref(&buf)?))?;data.extend_from_slice(&bytes[..n as usize]);}Ok(())})();
     let _=env.call_method(&stream,"close","()V",&[]);read?;
     if !data.is_empty(){let name=format!("exit-{stamp}.trace");if std::fs::write(dir.join(&name),data).is_ok(){trace_file=Some(name);}}
    }
   }
   Ok(serde_json::json!({"timestamp_ms":stamp,"reason":reason,"status":status,"description":description,"trace_file":trace_file}))
  })?;
            reports.push(row);
        }
        Ok(())
    });
    let report = serde_json::json!({"schema":1,"version":crate::backend::VERSION,"source_commit":option_env!("HORIZON_ANDROID_BUILD_COMMIT"),"exits":reports,"note":"Android reasons: 4 Java crash, 5 native crash, 6 ANR, 3 low memory. Missing traces mean the OS did not expose one. Reports may contain diagnostic paths or server addresses; inspect before sharing."});
    if let Ok(bytes) = serde_json::to_vec_pretty(&report) {
        let _ = std::fs::write(dir.join("android-exits.json"), bytes);
    }
    // Keep exported traces bounded independently of the number of app launches.
    if let Ok(files) = std::fs::read_dir(&dir) {
        let mut traces: Vec<_> = files
            .flatten()
            .filter(|f| {
                f.file_name().to_string_lossy().starts_with("exit-")
                    && f.path().extension().is_some_and(|s| s == "trace")
            })
            .collect();
        traces.sort_by_key(|f| f.file_name());
        let excess = traces.len().saturating_sub(8);
        for f in traces.into_iter().take(excess) {
            let _ = std::fs::remove_file(f.path());
        }
    }
    let crashes = home.join("crash-reports");
    if let Ok(entries) = std::fs::read_dir(crashes) {
        let mut files: Vec<_> = entries
            .flatten()
            .filter(|f| f.file_type().is_ok_and(|t| t.is_file()))
            .collect();
        files.sort_by_key(|f| f.file_name());
        for f in files.into_iter().rev().take(3) {
            if let Ok(data) = tail(&f.path()) {
                let _ = std::fs::write(dir.join(f.file_name()), data);
            }
        }
    }
}
pub fn install(home: &Path) {
    collect(home);
    let dir = home.join("performance");
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::write(
            dir.join("last-rust-panic.txt"),
            format!("{info}\n{}", std::backtrace::Backtrace::force_capture()),
        );
        old(info);
    }));
}

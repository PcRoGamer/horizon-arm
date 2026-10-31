//! GameActivity's Unicode text buffer is independent of winit's key events.
use horizon_client::osk::Output;
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JObject, JValue},
};
use std::cell::RefCell;
use winit::platform::android::activity::input::{
    ImeOptions, InputType, TextInputAction, TextInputState, TextSpan,
};

/// The originals remain outside the source tree and are supplied by this APK's
/// private asset bundle. Install after resource updates, before building the HUD.
pub fn install_bedrock_controls(home: &std::path::Path) -> horizon_client::Result<()> {
    use std::io::Read;
    let bytes = crate::APP.with(|slot| -> horizon_client::Result<Vec<u8>> {
        let app = slot.borrow();
        let app = app.as_ref().ok_or("Android activity unavailable")?;
        let mut asset = app
            .asset_manager()
            .open(c"bedrock-controls.zip")
            .ok_or("Original Bedrock controls missing from APK")?;
        let mut bytes = Vec::new();
        asset.read_to_end(&mut bytes)?;
        Ok(bytes)
    })?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let target = home
        .join(".local/horizon-client")
        .join(horizon_core::VANILLA_VERSION)
        .join("assets/minecraft/textures/gui/bedrock");
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let path = entry
            .enclosed_name()
            .ok_or("Invalid control texture path")?;
        if entry.is_dir() || path.extension().is_none_or(|x| x != "png") {
            continue;
        }
        if entry.size() > 1024 * 1024 {
            return Err("Control texture exceeds size limit".into());
        }
        let file = target.join(path);
        std::fs::create_dir_all(file.parent().unwrap())?;
        let temporary = file.with_extension("partial");
        let mut out = std::fs::File::create(&temporary)?;
        std::io::copy(&mut entry, &mut out)?;
        drop(out);
        std::fs::rename(temporary, file)?;
    }
    Ok(())
}

thread_local! {
    static EDIT: RefCell<Option<String>> = const { RefCell::new(None) };
    static VIBRATOR: RefCell<Option<Option<GlobalRef>>> = const { RefCell::new(None) };
    static VIBRATING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn with_vibrator(f: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> jni::errors::Result<()>) {
    crate::APP.with(|slot| {
        let app = slot.borrow();
        let Some(app) = app.as_ref() else {
            return;
        };
        // AndroidApp keeps the VM and activity alive for this callback.
        let Ok(vm) = (unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }) else {
            return;
        };
        let Ok(mut env) = vm.attach_current_thread() else {
            return;
        };
        let result = env.with_local_frame(16, |env| -> jni::errors::Result<()> {
            VIBRATOR.with(|cache| {
                let mut cache = cache.borrow_mut();
                if cache.is_none() {
                    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
                    let name = JObject::from(env.new_string("vibrator")?);
                    let service = env
                        .call_method(
                            &activity,
                            "getSystemService",
                            "(Ljava/lang/String;)Ljava/lang/Object;",
                            &[JValue::Object(&name)],
                        )?
                        .l()?;
                    let supported = !service.is_null()
                        && env.call_method(&service, "hasVibrator", "()Z", &[])?.z()?;
                    *cache = Some(if supported {
                        Some(env.new_global_ref(service)?)
                    } else {
                        None
                    });
                }
                if let Some(service) = cache.as_ref().and_then(|s| s.as_ref()) {
                    f(env, service.as_obj())?;
                }
                Ok(())
            })
        });
        if let Err(error) = result {
            // A device without a vibrator must not leave a pending Java exception.
            let _ = env.exception_clear();
            log::warn!("Android haptics: {error}");
        }
    });
}
pub fn vibrate(pulse: horizon_client::gamepad::Pulse) {
    let amplitude = (pulse.strong.max(pulse.weak).clamp(0.0, 1.0) * 255.0).round() as i32;
    if amplitude == 0 || pulse.millis == 0 {
        return;
    }
    with_vibrator(|env, service| {
        let effect = env
            .call_static_method(
                "android/os/VibrationEffect",
                "createOneShot",
                "(JI)Landroid/os/VibrationEffect;",
                &[
                    JValue::Long(i64::from(pulse.millis.min(1000))),
                    JValue::Int(amplitude),
                ],
            )?
            .l()?;
        env.call_method(
            service,
            "vibrate",
            "(Landroid/os/VibrationEffect;)V",
            &[JValue::Object(&effect)],
        )?;
        VIBRATING.with(|v| v.set(true));
        Ok(())
    });
}
pub fn cancel_vibration() {
    if !VIBRATING.with(|v| v.replace(false)) {
        return;
    }
    with_vibrator(|env, service| {
        env.call_method(service, "cancel", "()V", &[])?;
        Ok(())
    });
}

fn with_context<R>(
    f: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> jni::errors::Result<R>,
) -> Option<R> {
    crate::APP.with(|slot| {
        let app = slot.borrow();
        let app = app.as_ref()?;
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        let result = env.with_local_frame(16, |env| {
            let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
            f(env, &activity)
        });
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                let _ = env.exception_clear();
                log::warn!("Android clipboard: {error}");
                None
            }
        }
    })
}
pub fn clipboard_get() -> Option<String> {
    with_context(|env, activity| {
        let name = JObject::from(env.new_string("clipboard")?);
        let service = env
            .call_method(
                activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&name)],
            )?
            .l()?;
        let clip = env
            .call_method(
                &service,
                "getPrimaryClip",
                "()Landroid/content/ClipData;",
                &[],
            )?
            .l()?;
        if clip.is_null() || env.call_method(&clip, "getItemCount", "()I", &[])?.i()? == 0 {
            return Ok(String::new());
        }
        let item = env
            .call_method(
                &clip,
                "getItemAt",
                "(I)Landroid/content/ClipData$Item;",
                &[JValue::Int(0)],
            )?
            .l()?;
        // Only paste text. Do not resolve content URIs or read clipboard files.
        let text = env
            .call_method(&item, "getText", "()Ljava/lang/CharSequence;", &[])?
            .l()?;
        if text.is_null() {
            return Ok(String::new());
        }
        let text = jni::objects::JString::from(
            env.call_method(text, "toString", "()Ljava/lang/String;", &[])?
                .l()?,
        );
        let value = env.get_string(&text)?.into();
        Ok(value)
    })
}
pub fn clipboard_set(text: &str) {
    with_context(|env, activity| {
        let name = JObject::from(env.new_string("clipboard")?);
        let service = env
            .call_method(
                activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&name)],
            )?
            .l()?;
        let label = JObject::from(env.new_string("Horizon")?);
        let value = JObject::from(env.new_string(text)?);
        let clip = env
            .call_static_method(
                "android/content/ClipData",
                "newPlainText",
                "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;",
                &[JValue::Object(&label), JValue::Object(&value)],
            )?
            .l()?;
        env.call_method(
            service,
            "setPrimaryClip",
            "(Landroid/content/ClipData;)V",
            &[JValue::Object(&clip)],
        )?;
        Ok(())
    });
}

pub fn active() -> bool {
    EDIT.with(|e| e.borrow().is_some())
}

pub fn show(text: &str) {
    EDIT.with(|e| *e.borrow_mut() = Some(text.into()));
    crate::APP.with(|slot| {
        let app = slot.borrow();
        let app = app.as_ref().unwrap();
        app.set_ime_editor_info(
            InputType::TYPE_CLASS_TEXT | InputType::TYPE_TEXT_FLAG_MULTI_LINE,
            TextInputAction::None,
            ImeOptions::IME_FLAG_NO_FULLSCREEN | ImeOptions::IME_FLAG_NO_ENTER_ACTION,
        );
        // The Java Editable stores selection offsets in UTF-16 code units.
        app.set_text_input_state(TextInputState {
            text: text.into(),
            selection: TextSpan {
                start: text.encode_utf16().count(),
                end: text.encode_utf16().count(),
            },
            compose_region: None,
        });
        app.show_soft_input(false);
    });
}

pub fn hide() {
    EDIT.with(|e| *e.borrow_mut() = None);
    crate::APP.with(|slot| {
        if let Some(app) = slot.borrow().as_ref() {
            app.hide_soft_input(false);
        }
    });
}

pub fn poll() -> Vec<Output> {
    EDIT.with(|edit| {
        let mut edit = edit.borrow_mut();
        let Some(old) = edit.as_mut() else {
            return vec![];
        };
        let state = crate::APP.with(|a| a.borrow().as_ref().unwrap().text_input_state());
        let new = state.text;
        if new == *old {
            return vec![];
        }
        let mut out = horizon_client::ime_edit::changes(old, &new);
        *old = new;
        if old.ends_with('\n') {
            // Single-line fields treat a keyboard newline as confirmation.
            if let Some(Output::Text(t)) = out.last_mut() {
                t.pop();
            }
            out.push(Output::Enter);
        }
        out
    })
}

/// Field hit tests use the same GUI coordinates as the touch menus.
pub fn field_at(p: [f32; 2]) -> bool {
    horizon_client::widgets::last_targets().iter().any(|t| {
        let [x, y, w, h] = t.rect;
        t.kind == horizon_client::widgets::Kind::Field
            && (x..x + w).contains(&p[0])
            && (y..y + h).contains(&p[1])
    })
}

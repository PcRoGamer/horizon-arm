//! Optional native frame-rate requests, gyro and ADPF. No API-30+ load-time symbols.
#[path = "device_math.rs"]
mod math;
use std::{
    cell::RefCell,
    ffi::{c_char, c_int, c_void},
    time::Instant,
};
#[link(name = "dl")]
unsafe extern "C" {
    fn dlopen(n: *const c_char, f: c_int) -> *mut c_void;
    fn dlsym(h: *mut c_void, n: *const c_char) -> *mut c_void;
    fn gettid() -> c_int;
}
#[link(name = "android")]
unsafe extern "C" {
    fn ALooper_prepare(flags: c_int) -> *mut c_void;
    fn ASensorManager_getInstance() -> *mut c_void;
    fn ASensorManager_getDefaultSensor(m: *mut c_void, t: c_int) -> *const c_void;
    fn ASensorManager_createEventQueue(
        m: *mut c_void,
        l: *mut c_void,
        id: c_int,
        cb: *mut c_void,
        data: *mut c_void,
    ) -> *mut c_void;
    fn ASensorEventQueue_enableSensor(q: *mut c_void, s: *const c_void) -> c_int;
    fn ASensorEventQueue_disableSensor(q: *mut c_void, s: *const c_void) -> c_int;
    fn ASensorEventQueue_setEventRate(q: *mut c_void, s: *const c_void, rate: c_int) -> c_int;
    fn ASensorEventQueue_getEvents(q: *mut c_void, e: *mut SensorEvent, n: usize) -> isize;
}
#[repr(C)]
struct SensorEvent {
    version: i32,
    sensor: i32,
    kind: i32,
    reserved: i32,
    timestamp: i64,
    data: [f32; 16],
    flags: u32,
    tail: [i32; 3],
}
struct Runtime {
    lib: *mut c_void,
    window: usize,
    hz: u16,
    rate_status: i32,
    queue: *mut c_void,
    sensor: *const c_void,
    gyro_on: bool,
    last_sensor: i64,
    session: *mut c_void,
    hint_failed: bool,
    target: i64,
    gyro_samples: u64,
    adpf_reports: u64,
    adpf_status: i32,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            lib: unsafe { dlopen(c"libandroid.so".as_ptr(), 2) },
            window: 0,
            hz: u16::MAX,
            rate_status: -1,
            queue: std::ptr::null_mut(),
            sensor: std::ptr::null(),
            gyro_on: false,
            last_sensor: 0,
            session: std::ptr::null_mut(),
            hint_failed: false,
            target: 0,
            gyro_samples: 0,
            adpf_reports: 0,
            adpf_status: 0,
        }
    }
}
thread_local! {static STATE:RefCell<Runtime>=RefCell::new(Runtime::default());}
impl Runtime {
    unsafe fn symbol(&self, n: &std::ffi::CStr) -> *mut c_void {
        if self.lib.is_null() {
            std::ptr::null_mut()
        } else {
            unsafe { dlsym(self.lib, n.as_ptr()) }
        }
    }
    fn close_hint(&mut self) {
        if !self.session.is_null() {
            unsafe {
                let p = self.symbol(c"APerformanceHint_closeSession");
                if !p.is_null() {
                    let f: unsafe extern "C" fn(*mut c_void) = std::mem::transmute(p);
                    f(self.session);
                }
            }
            self.session = std::ptr::null_mut();
        }
    }
}
pub struct Frame {
    start: Instant,
    enabled: bool,
}
/// The actual duration is CPU-side frame work including submission waits, not GPU execution time.
pub fn frame(enabled: bool, hz: u16, active: bool) -> Frame {
    STATE.with(|slot| {
        let mut r = slot.borrow_mut();
        crate::APP.with(|app| {
            if let Some(w) = app.borrow().as_ref().and_then(|a| a.native_window()) {
                let ptr = w.ptr().as_ptr() as usize;
                let requested = if active { hz } else { 0 };
                if r.window != ptr || r.hz != requested {
                    unsafe {
                        let p = r.symbol(c"ANativeWindow_setFrameRate");
                        r.rate_status = if p.is_null() {
                            -38
                        } else {
                            let f: unsafe extern "C" fn(*mut c_void, f32, i8) -> i32 =
                                std::mem::transmute(p);
                            f(ptr as *mut c_void, f32::from(requested), 0)
                        };
                    }
                    r.window = ptr;
                    r.hz = requested;
                }
            }
        });
        if !enabled || !active {
            r.close_hint();
            r.hint_failed = false;
            return;
        }
        // Use an explicit frame budget; never infer it from a slow frame itself.
        let target = 1_000_000_000i64 / i64::from(if hz == 0 { 60 } else { hz });
        unsafe {
            if r.session.is_null() && !r.hint_failed {
                let get = r.symbol(c"APerformanceHint_getManager");
                let create = r.symbol(c"APerformanceHint_createSession");
                if !get.is_null() && !create.is_null() {
                    let get: unsafe extern "C" fn() -> *mut c_void = std::mem::transmute(get);
                    let create: unsafe extern "C" fn(
                        *mut c_void,
                        *const i32,
                        usize,
                        i64,
                    ) -> *mut c_void = std::mem::transmute(create);
                    let manager = get();
                    if !manager.is_null() {
                        r.session = create(manager, &gettid(), 1, target);
                    }
                }
                r.hint_failed = r.session.is_null();
                r.target = target;
            }
            if !r.session.is_null() && r.target != target {
                let p = r.symbol(c"APerformanceHint_updateTargetWorkDuration");
                if !p.is_null() {
                    let f: unsafe extern "C" fn(*mut c_void, i64) -> i32 = std::mem::transmute(p);
                    f(r.session, target);
                }
                r.target = target;
            }
        }
    });
    Frame {
        start: Instant::now(),
        enabled: enabled && active,
    }
}
impl Drop for Frame {
    fn drop(&mut self) {
        if self.enabled {
            STATE.with(|s| {
                let mut r = s.borrow_mut();
                if !r.session.is_null() {
                    unsafe {
                        let p = r.symbol(c"APerformanceHint_reportActualWorkDuration");
                        if !p.is_null() {
                            let f: unsafe extern "C" fn(*mut c_void, i64) -> i32 =
                                std::mem::transmute(p);
                            r.adpf_status = f(
                                r.session,
                                self.start.elapsed().as_nanos().min(i64::MAX as u128) as i64,
                            );
                            r.adpf_reports += 1;
                        }
                    }
                }
            });
        }
    }
}
/// Gyro uses Android device axes, remapped by current display rotation.
pub fn gyro(enabled: bool, sensitivity: f32, rotation: i32) -> [f32; 2] {
    STATE.with(|slot| {
        let mut r = slot.borrow_mut();
        let mut out = [0.; 2];
        unsafe {
            if enabled && r.queue.is_null() {
                let manager = ASensorManager_getInstance();
                if !manager.is_null() {
                    r.sensor = ASensorManager_getDefaultSensor(manager, 4);
                    if !r.sensor.is_null() {
                        r.queue = ASensorManager_createEventQueue(
                            manager,
                            ALooper_prepare(1),
                            907,
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                        );
                    }
                }
            }
            if r.queue.is_null() {
                return out;
            }
            if r.gyro_on != enabled {
                if enabled {
                    if ASensorEventQueue_enableSensor(r.queue, r.sensor) < 0 {
                        return out;
                    }
                    ASensorEventQueue_setEventRate(r.queue, r.sensor, 10_000);
                } else {
                    ASensorEventQueue_disableSensor(r.queue, r.sensor);
                }
                r.gyro_on = enabled;
                r.last_sensor = 0;
            }
            for _ in 0..128 {
                let mut event: SensorEvent = std::mem::zeroed();
                if ASensorEventQueue_getEvents(r.queue, &mut event, 1) <= 0 {
                    break;
                }
                if !enabled || event.kind != 4 {
                    continue;
                }
                let dt = (event.timestamp - r.last_sensor) as f32 * 1e-9;
                let valid = r.last_sensor != 0 && dt > 0. && dt < 0.1;
                r.last_sensor = event.timestamp;
                if !valid {
                    continue;
                }
                r.gyro_samples += 1;
                let delta = math::gyro_delta(
                    [event.data[0], event.data[1], event.data[2]],
                    dt,
                    rotation,
                    sensitivity,
                );
                out[0] += delta[0];
                out[1] += delta[1];
            }
        }
        out.map(|v| v.clamp(-20., 20.))
    })
}
pub fn diagnostics() -> serde_json::Value {
    STATE.with(|s|{let r=s.borrow();serde_json::json!({"display_hz_requested":r.hz,"display_request_result":r.rate_status,"adpf_session_active":!r.session.is_null(),"adpf_unavailable":r.hint_failed,"gyro_available":!r.sensor.is_null(),"gyro_active":r.gyro_on,"gyro_samples":r.gyro_samples,"adpf_report_count":r.adpf_reports,"adpf_report_status":r.adpf_status})})
}

/// Release sensor delivery and hint sessions immediately when the window loses focus.
pub fn suspend() {
    let _ = gyro(false, 1.0, 0);
    let _ = frame(false, 0, false);
}

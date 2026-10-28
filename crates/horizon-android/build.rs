//! `liblog` for `__android_log_write` (src/lib.rs `say`). Nothing on a desktop
//! target, where this crate is empty.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-link-lib=log");
        println!("cargo:rustc-link-lib=android");
    }
}

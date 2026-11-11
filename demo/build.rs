fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_BACKEND_WINDOWS");
    #[cfg(all(target_os = "windows", feature = "backend-windows"))]
    panechord_windows::deployment::configure_self_contained();
}

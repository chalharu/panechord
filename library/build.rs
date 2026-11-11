fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_WINDOWS_SELF_CONTAINED");
    #[cfg(all(target_os = "windows", feature = "windows-self-contained"))]
    panechord_windows::deployment::configure_self_contained();
}

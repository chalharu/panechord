//! Pure asset regression tests; importing the build script never runs main().
//! Native application/executor tests still belong on an isolated test desktop.
#[allow(dead_code)]
#[path = "../build.rs"]
mod build_script;

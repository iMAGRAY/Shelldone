pub fn shelldone_version() -> &'static str {
    // See build.rs
    env!("SHELLDONE_CI_TAG")
}

pub fn shelldone_target_triple() -> &'static str {
    // See build.rs
    env!("SHELLDONE_TARGET_TRIPLE")
}

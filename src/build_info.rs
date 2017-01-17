//! Information about the build-environment

// Provides
//  pub const GIT_VERSION: Option<&'static str>
//  pub const PKG_VERSION: &'static str
//  pub const TARGET: &'static str
//  pub const PROFILE: &'static str
//  pub const DEPENDENCIES: [(&'static str, &'static str); _]
//  pub const FEATURES: [&'static str; _]
include!(concat!(env!("OUT_DIR"), "/build_info.include"));


// TODO just serialize those in build.rs ?

pub fn get_dependencies_string() -> String {
    DEPENDENCIES.iter().map(|&(ref pkg, ref ver)| format!("{} {}", pkg, ver))
                .collect::<Vec<_>>().join(", ")
}

pub fn get_features_string() -> String {
    FEATURES.join(", ")
}

use std::{os::unix::ffi::OsStrExt, path::Path};

use anyhow::Result;

#[cxx::bridge]
pub(crate) mod ffi {
    unsafe extern "C++" {
        include!("valhalla/src/config.hpp");

        #[namespace = "boost::property_tree"]
        type ptree;
        fn from_file(path: &[u8]) -> Result<UniquePtr<ptree>>;
        fn from_json(config: &str) -> Result<UniquePtr<ptree>>;
    }
}

/// Wrapper around Valhalla configuration.
///
/// Provides methods to read configuration from a file or JSON string, created by `valhalla_build_config` script.
/// For more information about the configuration and available options see the Valhalla documentation:
/// https://github.com/valhalla/valhalla/blob/master/scripts/valhalla_build_config
pub struct Config(cxx::UniquePtr<ffi::ptree>);

impl Config {
    /// Reads configuration from the given Valhalla configuration file.
    /// ```rust
    /// let config = valhalla::Config::from_file("path/to/config.json");
    /// ```
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Config(ffi::from_file(
            path.as_ref().as_os_str().as_bytes(),
        )?))
    }

    /// Reads configuration from Valhalla configuration JSON string.
    /// ```rust
    /// let json = r#"{"mjolnir":{"tile_extract":"path/to/tiles.tar","traffic_extract":"path/to/traffic.tar"}}"#;
    /// let config = valhalla::Config::from_json(&json);
    /// ```
    pub fn from_json(config: &str) -> Result<Self> {
        Ok(Config(ffi::from_json(config)?))
    }

    /// Creates a new Valhalla configuration from path to the tiles tar extract.
    /// ```rust
    /// let config = valhalla::Config::from_tile_extract("path/to/tiles.tar");
    /// ```
    pub fn from_tile_extract(tile_extract: impl AsRef<Path>) -> Result<Self> {
        let config = format!(
            "{{\"mjolnir\":{{\"tile_extract\":\"{}\"}}}}",
            tile_extract.as_ref().display()
        );
        Self::from_json(&config)
    }

    /// Reference to the inner Valhalla configuration object.
    pub(crate) fn inner(&self) -> &ffi::ptree {
        self.0.as_ref().unwrap()
    }
}

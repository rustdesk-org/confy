//! Zero-boilerplate configuration management
//!
//! ## Why?
//!
//! There are a lot of different requirements when
//! selecting, loading and writing a config,
//! depending on the operating system and other
//! environment factors.
//!
//! In many applications this burden is left to you,
//! the developer of an application, to figure out
//! where to place the configuration files.
//!
//! This is where `confy` comes in.
//!
//! ## Idea
//!
//! `confy` takes care of figuring out operating system
//! specific and environment paths before reading and
//! writing a configuration.
//!
//! It gives you easy access to a configuration file
//! which is mirrored into a Rust `struct` via [serde].
//! This way you only need to worry about the layout of
//! your configuration, not where and how to store it.
//!
//! [serde]: https://docs.rs/serde
//!
//! `confy` uses the [`Default`] trait in Rust to automatically
//! create a new configuration, if none is available to read
//! from yet.
//! This means that you can simply assume your application
//! to have a configuration, which will be created with
//! default values of your choosing, without requiring
//! any special logic to handle creation.
//!
//! [`Default`]: https://doc.rust-lang.org/std/default/trait.Default.html
//!
//! ```rust,no_run
//! use serde_derive::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct MyConfig {
//!     version: u8,
//!     api_key: String,
//! }
//!
//! /// `MyConfig` implements `Default`
//! impl ::std::default::Default for MyConfig {
//!     fn default() -> Self { Self { version: 0, api_key: "".into() } }
//! }
//!
//! fn main() -> Result<(), confy::ConfyError> {
//!     let cfg = confy::load("my-app-name", None)?;
//!     Ok(())
//! }
//! ```
//!
//! Updating the configuration is then done via the [`store`] function.
//!
//! [`store`]: fn.store.html
//!

mod utils;
use utils::*;

use directories_next::ProjectDirs;
use serde::{de::DeserializeOwned, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[cfg(not(any(feature = "toml_conf", feature = "yaml_conf")))]
compile_error!(
    "Exactly one config language feature must be enabled to use \
confy.  Please enable one of either the `toml_conf` or `yaml_conf` \
features."
);

#[cfg(all(feature = "toml_conf", feature = "yaml_conf"))]
compile_error!(
    "Exactly one config language feature must be enabled to compile \
confy.  Please disable one of either the `toml_conf` or `yaml_conf` features. \
NOTE: `toml_conf` is a default feature, so disabling it might mean switching off \
default features for confy in your Cargo.toml"
);

#[cfg(feature = "toml_conf")]
const EXTENSION: &str = "toml";

#[cfg(feature = "yaml_conf")]
const EXTENSION: &str = "yml";

/// The errors the confy crate can encounter.
#[derive(Debug, Error)]
pub enum ConfyError {
    #[cfg(feature = "toml_conf")]
    #[error("Bad TOML data")]
    BadTomlData(#[source] toml::de::Error),

    #[cfg(feature = "yaml_conf")]
    #[error("Bad YAML data")]
    BadYamlData(#[source] serde_yaml::Error),

    #[error("Failed to create directory")]
    DirectoryCreationFailed(#[source] std::io::Error),

    #[error("Failed to load configuration file")]
    GeneralLoadError(#[source] std::io::Error),

    #[error("Bad configuration directory: {0}")]
    BadConfigDirectory(String),

    #[cfg(feature = "toml_conf")]
    #[error("Failed to serialize configuration data into TOML")]
    SerializeTomlError(#[source] toml::ser::Error),

    #[cfg(feature = "yaml_conf")]
    #[error("Failed to serialize configuration data into YAML")]
    SerializeYamlError(#[source] serde_yaml::Error),

    #[error("Failed to write configuration file")]
    WriteConfigurationFileError(#[source] std::io::Error),

    #[error("Failed to read configuration file")]
    ReadConfigurationFileError(#[source] std::io::Error),

    #[error("Failed to open configuration file")]
    OpenConfigurationFileError(#[source] std::io::Error),
}

/// Load an application configuration from disk
///
/// A new configuration file is created with default values if none
/// exists.
///
/// Errors that are returned from this function are I/O related,
/// for example if the writing of the new configuration fails
/// or `confy` encounters an operating system or environment
/// that it does not support.
///
/// **Note:** The type of configuration needs to be declared in some way
/// that is inferrable by the compiler. Also note that your
/// configuration needs to implement `Default`.
///
/// ```rust,no_run
/// # use confy::ConfyError;
/// # use serde_derive::{Serialize, Deserialize};
/// # fn main() -> Result<(), ConfyError> {
/// #[derive(Default, Serialize, Deserialize)]
/// struct MyConfig {}
///
/// let cfg: MyConfig = confy::load("my-app-name", None)?;
/// # Ok(())
/// # }
/// ```
pub fn load<'a, T: Serialize + DeserializeOwned + Default>(
    app_name: &str,
    config_name: impl Into<Option<&'a str>>,
) -> Result<T, ConfyError> {
    get_configuration_file_path(app_name, config_name).and_then(load_path)
}

/// Load an application configuration from a specified path.
///
/// A new configuration file is created with default values if none
/// exists.
///
/// This is an alternate version of [`load`] that allows the specification of
/// an arbitrary path instead of a system one.  For more information on errors
/// and behavior, see [`load`]'s documentation.
///
/// [`load`]: fn.load.html
pub fn load_path<T: Serialize + DeserializeOwned + Default>(
    path: impl AsRef<Path>,
) -> Result<T, ConfyError> {
    match File::open(&path) {
        Ok(mut cfg) => {
            let cfg_string = cfg
                .get_string()
                .map_err(ConfyError::ReadConfigurationFileError)?;

            #[cfg(feature = "toml_conf")]
            {
                let cfg_data = toml::from_str(&cfg_string);
                cfg_data.map_err(ConfyError::BadTomlData)
            }
            #[cfg(feature = "yaml_conf")]
            {
                let cfg_data = serde_yaml::from_str(&cfg_string);
                cfg_data.map_err(ConfyError::BadYamlData)
            }
        }
        Err(e) => Err(ConfyError::GeneralLoadError(e)),
    }
}

/// Save changes made to a configuration object
///
/// This function will update a configuration,
/// with the provided values, and create a new one,
/// if none exists.
///
/// You can also use this function to create a new configuration
/// with different initial values than which are provided
/// by your `Default` trait implementation, or if your
/// configuration structure _can't_ implement `Default`.
///
/// ```rust,no_run
/// # use serde_derive::{Serialize, Deserialize};
/// # use confy::ConfyError;
/// # fn main() -> Result<(), ConfyError> {
/// #[derive(Serialize, Deserialize)]
/// struct MyConf {}
///
/// let my_cfg = MyConf {};
/// confy::store("my-app-name", None, my_cfg)?;
/// # Ok(())
/// # }
/// ```
///
/// Errors returned are I/O errors related to not being
/// able to write the configuration file or if `confy`
/// encounters an operating system or environment it does
/// not support.
pub fn store<'a, T: Serialize>(
    app_name: &str,
    config_name: impl Into<Option<&'a str>>,
    cfg: T,
) -> Result<(), ConfyError> {
    let path = get_configuration_file_path(app_name, config_name)?;
    store_path(path, cfg)
}

/// Save changes made to a configuration object at a specified path
///
/// This is an alternate version of [`store`] that allows the specification of
/// an arbitrary path instead of a system one.  For more information on errors
/// and behavior, see [`store`]'s documentation.
///
/// [`store`]: fn.store.html
pub fn store_path<T: Serialize>(path: impl AsRef<Path>, cfg: T) -> Result<(), ConfyError> {
    let path = path.as_ref();
    let config_dir = path
        .parent()
        .ok_or_else(|| ConfyError::BadConfigDirectory(format!("{:?} is a root or prefix", path)))?;
    fs::create_dir_all(config_dir).map_err(ConfyError::DirectoryCreationFailed)?;

    let s;
    #[cfg(feature = "toml_conf")]
    {
        s = toml::to_string_pretty(&cfg).map_err(ConfyError::SerializeTomlError)?;
    }
    #[cfg(feature = "yaml_conf")]
    {
        s = serde_yaml::to_string(&cfg).map_err(ConfyError::SerializeYamlError)?;
    }

    let mut path_tmp = path.to_path_buf();
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut i = 0;
    loop {
        i += 1;
        path_tmp.set_extension(format!(
            "{}_{:?}_{}",
            std::process::id(),
            std::thread::current().id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|x| x.as_nanos())
                .unwrap_or(i)
        ));
        if !path_tmp.exists() {
            break;
        }
    }
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path_tmp)
        .map_err(ConfyError::OpenConfigurationFileError)?;

    f.write_all(s.as_bytes())
        .map_err(ConfyError::WriteConfigurationFileError)?;
    f.flush().map_err(ConfyError::WriteConfigurationFileError)?;
    drop(f);
    std::fs::rename(path_tmp, path).map_err(ConfyError::WriteConfigurationFileError)?;
    Ok(())
}

/// Get the configuration file path used by [`load`] and [`store`]
///
/// This is useful if you want to show where the configuration file is to your user.
///
/// [`load`]: fn.load.html
/// [`store`]: fn.store.html
pub fn get_configuration_file_path<'a>(
    app_name: &str,
    config_name: impl Into<Option<&'a str>>,
) -> Result<PathBuf, ConfyError> {
    let config_name = config_name.into().unwrap_or("default-config");
    let project = ProjectDirs::from("rs", "", app_name).ok_or_else(|| {
        ConfyError::BadConfigDirectory("could not determine home directory path".to_string())
    })?;

    let config_dir_str = get_configuration_directory_str(&project)?;

    let path = [config_dir_str, &format!("{}.{}", config_name, EXTENSION)]
        .iter()
        .collect();

    Ok(path)
}

fn get_configuration_directory_str(project: &ProjectDirs) -> Result<&str, ConfyError> {
    let path = project.config_dir();
    path.to_str()
        .ok_or_else(|| ConfyError::BadConfigDirectory(format!("{:?} is not valid Unicode", path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serializer;
    use serde_derive::{Deserialize, Serialize};

    #[derive(PartialEq, Default, Debug, Serialize, Deserialize)]
    struct ExampleConfig {
        name: String,
        count: usize,
    }

    /// Run a test function with a temporary config path as fixture.
    fn with_config_path(test_fn: fn(&Path)) {
        let config_dir = tempfile::tempdir().expect("creating test fixture failed");
        // config_path should roughly correspond to the result of `get_configuration_file_path("example-app", "example-config")`
        let config_path = config_dir
            .path()
            .join("example-app")
            .join("example-config")
            .with_extension(EXTENSION);
        test_fn(&config_path);
        config_dir.close().expect("removing test fixture failed");
    }

    /// [`load_path`] loads [`ExampleConfig`].
    #[test]
    fn load_path_works() {
        with_config_path(|path| {
            let config: ExampleConfig = load_path(path).expect("load_path failed");
            assert_eq!(config, ExampleConfig::default());
        })
    }

    /// [`store_path`] stores [`ExampleConfig`].
    #[test]
    fn test_store_path() {
        with_config_path(|path| {
            let config: ExampleConfig = ExampleConfig {
                name: "Test".to_string(),
                count: 42,
            };
            store_path(path, &config).expect("store_path failed");
            let loaded = load_path(path).expect("load_path failed");
            assert_eq!(config, loaded);
        })
    }

    /// [`store_path`] fails when given a root path.
    #[test]
    fn test_store_path_root_error() {
        let err = store_path(PathBuf::from("/"), &ExampleConfig::default())
            .expect_err("store_path should fail");
        assert_eq!(
            err.to_string(),
            r#"Bad configuration directory: "/" is a root or prefix"#,
        )
    }

    struct CannotSerialize;

    impl Serialize for CannotSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            use serde::ser::Error;
            Err(S::Error::custom("cannot serialize CannotSerialize"))
        }
    }

    /// Verify that if you call store_path() with an object that fails to serialize,
    /// the file on disk will not be overwritten or truncated.
    #[test]
    fn test_store_path_atomic() -> Result<(), ConfyError> {
        let tmp = tempfile::NamedTempFile::new().expect("Failed to create NamedTempFile");
        let path = tmp.path();
        let message = "Hello world!";

        // Write to file.
        {
            let mut f = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .map_err(ConfyError::OpenConfigurationFileError)?;

            f.write_all(message.as_bytes())
                .map_err(ConfyError::WriteConfigurationFileError)?;

            f.flush().map_err(ConfyError::WriteConfigurationFileError)?;
        }

        // Call store_path() to overwrite file with an object that fails to serialize.
        let store_result = store_path(path, CannotSerialize);
        assert!(matches!(store_result, Err(_)));

        // Ensure file was not overwritten.
        let buf = {
            let mut f = OpenOptions::new()
                .read(true)
                .open(path)
                .map_err(ConfyError::OpenConfigurationFileError)?;

            let mut buf = String::new();

            use std::io::Read;
            f.read_to_string(&mut buf)
                .map_err(ConfyError::ReadConfigurationFileError)?;
            buf
        };

        assert_eq!(buf, message);
        Ok(())
    }
}

//! Where settings and the local database live on each platform.

use std::io;
use std::path::PathBuf;

/// Directories holding `settings.json` and `cinebox.sqlite`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    pub config: PathBuf,
    pub data: PathBuf,
}

impl AppDirs {
    /// Next to the executable, so the unpacked folder stays portable.
    ///
    /// # Errors
    ///
    /// The executable path cannot be resolved.
    #[cfg(target_os = "windows")]
    pub fn system() -> io::Result<Self> {
        let dir = exe_dir()?;

        Ok(Self {
            config: dir.clone(),
            data: dir,
        })
    }

    /// XDG base directories. The executable's folder may be read-only
    /// (AppImage mount, `/usr/bin`), so nothing is written next to it.
    ///
    /// # Errors
    ///
    /// Neither the XDG variables nor `HOME` give an absolute path.
    #[cfg(target_os = "linux")]
    pub fn system() -> io::Result<Self> {
        xdg_dirs(
            std::env::var_os("XDG_CONFIG_HOME"),
            std::env::var_os("XDG_DATA_HOME"),
            std::env::var_os("HOME"),
        )
    }

    /// The app's internal storage, handed over by the Android entry point
    /// through [`set_android_root`].
    ///
    /// # Errors
    ///
    /// [`set_android_root`] was not called.
    #[cfg(target_os = "android")]
    pub fn system() -> io::Result<Self> {
        let Some(root) = ANDROID_ROOT.get() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "the Android internal storage path was not set",
            ));
        };

        Ok(Self {
            config: root.join("config"),
            data: root.join("data"),
        })
    }

    /// # Errors
    ///
    /// Always: no storage location is defined for this platform.
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "android")))]
    pub fn system() -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no settings or data location is defined for this platform",
        ))
    }
}

#[cfg(target_os = "android")]
static ANDROID_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Android has no fixed per-user directory; the activity knows its internal
/// storage path and passes it here before anything reads settings.
#[cfg(target_os = "android")]
pub fn set_android_root(root: PathBuf) {
    let _ = ANDROID_ROOT.set(root);
}

/// Directory that contains the running executable (or the test binary).
///
/// # Errors
///
/// [`std::env::current_exe`] failed, or the path has no parent.
pub fn exe_dir() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    match exe.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => Ok(dir.to_path_buf()),
        _ => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "executable has no parent directory",
        )),
    }
}

#[cfg(target_os = "linux")]
const XDG_APP_DIR: &str = "cinebox";

/// Per the XDG base directory spec, unset or relative values fall back to
/// `$HOME/.config` and `$HOME/.local/share`.
#[cfg(target_os = "linux")]
fn xdg_dirs(
    config_home: Option<std::ffi::OsString>,
    data_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> io::Result<AppDirs> {
    let home = absolute(home);
    let resolve = |explicit: Option<std::ffi::OsString>, fallback: &str| {
        if let Some(dir) = absolute(explicit) {
            return Ok(dir);
        }

        home.as_ref()
            .map(|home| home.join(fallback))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
    };

    Ok(AppDirs {
        config: resolve(config_home, ".config")?.join(XDG_APP_DIR),
        data: resolve(data_home, ".local/share")?.join(XDG_APP_DIR),
    })
}

#[cfg(target_os = "linux")]
fn absolute(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|path| path.is_absolute())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    mod xdg_dirs {
        use super::*;

        #[test]
        fn defaults_to_home_config_and_local_share() -> io::Result<()> {
            let dirs = xdg_dirs(None, None, Some("/home/u".into()))?;

            assert_eq!(
                dirs,
                AppDirs {
                    config: PathBuf::from("/home/u/.config/cinebox"),
                    data: PathBuf::from("/home/u/.local/share/cinebox"),
                }
            );
            Ok(())
        }

        #[test]
        fn honours_absolute_xdg_variables() -> io::Result<()> {
            let dirs = xdg_dirs(Some("/cfg".into()), Some("/data".into()), None)?;

            assert_eq!(
                dirs,
                AppDirs {
                    config: PathBuf::from("/cfg/cinebox"),
                    data: PathBuf::from("/data/cinebox"),
                }
            );
            Ok(())
        }

        #[test]
        fn ignores_relative_xdg_variables() -> io::Result<()> {
            let dirs = xdg_dirs(Some("cfg".into()), None, Some("/home/u".into()))?;

            assert_eq!(dirs.config, PathBuf::from("/home/u/.config/cinebox"));
            Ok(())
        }

        #[test]
        fn fails_without_home_or_xdg_variables() {
            assert!(xdg_dirs(None, None, None).is_err());
        }
    }
}

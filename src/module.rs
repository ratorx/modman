extern crate clap;
extern crate failure;
extern crate toml;

use dirs;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, Error};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process;
use std::vec::Vec;

static CONFIG_FILE: &'static str = "config.toml";
static INIT_SCRIPT: &'static str = "init.sh";
static CLEANUP_SCRIPT: &'static str = "cleanup.sh";
static PERMISSIONS_RX: u32 = 0b101;
static PERMISSIONS_R: u32 = 0b100;

#[derive(Deserialize, Debug)]
struct ModuleDef {
    description: Option<String>,

    #[serde(default)]
    init: bool,

    #[serde(default)]
    cleanup: bool,

    resources: HashMap<String, String>,
}

impl ModuleDef {
    fn new<P: AsRef<Path>>(module_path: P) -> Result<ModuleDef, ModuleError> {
        let buf = fs::read(module_path.as_ref().join(CONFIG_FILE))
            .map_err(|err| ModuleError::IO(file_name_to_string(module_path.as_ref()), err))?;
        let module_definition: ModuleDef = toml::from_slice(&buf)
            .map_err(|err| ModuleError::Parse(file_name_to_string(module_path.as_ref()), err))?;
        module_definition.verify(module_path)?;
        Ok(module_definition)
    }

    fn verify<P: AsRef<Path>>(&self, module_path: P) -> Result<(), ModuleError> {
        if self.init {
            let init_script_path = module_path.as_ref().join(INIT_SCRIPT);
            if !init_script_path.exists() || !check_permissions(
                init_script_path.metadata().unwrap().permissions().mode(),
                PERMISSIONS_RX,
            ) {
                return Err(ModuleError::Script(
                    file_name_to_string(module_path.as_ref()),
                    "init".to_owned(),
                ));
            }
        }

        if self.cleanup {
            let cleanup_script_path = module_path.as_ref().join(CLEANUP_SCRIPT);
            if !cleanup_script_path.exists() || !check_permissions(
                cleanup_script_path.metadata().unwrap().permissions().mode(),
                PERMISSIONS_RX,
            ) {
                return Err(ModuleError::Script(
                    file_name_to_string(module_path.as_ref()),
                    "cleanup".to_owned(),
                ));
            }
        }

        for resource in self.resources.keys() {
            let resource_path = module_path.as_ref().join(resource);
            if !resource_path.exists() || !check_permissions(
                resource_path.metadata().unwrap().permissions().mode(),
                PERMISSIONS_R,
            ) {
                return Err(ModuleError::Resource(
                    file_name_to_string(module_path.as_ref()),
                    resource.to_owned(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Fail)]
pub enum ModuleError {
    #[fail(
        display = "Module {}: Resource {} not found or incorrect permissions",
        _0,
        _1
    )]
    Resource(String, String),
    #[fail(
        display = "Module {}: {} script not found or has incorrect permissions",
        _0,
        _1
    )]
    Script(String, String),
    #[fail(
        display = "Module {}: {} script returned non-zero code",
        _0,
        _1
    )]
    Exec(String, String),
    #[fail(
        display = "Module {}: Existing file {} found; Use -f to force overwrite",
        _0,
        _1
    )]
    Install(String, String),
    #[fail(
        display = "Module {}: {} is not a directory; Use -f to force overwrite",
        _0,
        _1
    )]
    InstallPath(String, String),
    #[fail(
        display = "Module {}: {} is not a symlink or does not point to correct resource; Use -f to force deletion",
        _0,
        _1
    )]
    Uninstall(String, String),
    #[fail(display = "Module {}: {}", _0, _1)]
    Parse(String, toml::de::Error),
    #[fail(display = "Module {}: {}", _0, _1)]
    IO(String, io::Error),
    #[fail(display = "Module directory not found or has invalid permissions")]
    Directory,
}

#[derive(Debug)]
pub struct Module {
    path: PathBuf,
    definition: ModuleDef,
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.definition.description {
            None => write!(f, "{}", self.name()),
            Some(desc) => write!(f, "{} - {}", self.name(), desc),
        }
    }
}

impl Module {
    pub fn new<P: AsRef<Path>>(module_path: P) -> Result<Module, ModuleError> {
        let module_def = ModuleDef::new(module_path.as_ref())?;
        Ok(Module {
            path: module_path.as_ref().to_path_buf(),
            definition: module_def,
        })
    }

    pub fn name(&self) -> &str {
        self.path.file_name().unwrap().to_str().unwrap()
    }

    fn read_dir<P: AsRef<Path>>(module_dir: P) -> Result<fs::ReadDir, ModuleError> {
        fs::read_dir(module_dir).map_err(|_| ModuleError::Directory)
    }

    fn wrap_io_error(&self, err: Error) -> ModuleError {
        ModuleError::IO(self.name().to_owned(), err)
    }

    fn verify_module_creation<P: AsRef<Path>>(path: P) -> Result<(), PathBuf> {
        for p in path.as_ref().ancestors() {
            if p.exists() && !p.is_dir() {
                return Err(p.to_path_buf());
            }
        }
        Ok(())
    }

    pub fn list<P: AsRef<Path>>(
        module_dir: P,
    ) -> Result<Vec<Result<Module, ModuleError>>, ModuleError> {
        let iter = Module::read_dir(module_dir)?;
        let mut modules: Vec<Result<Module, ModuleError>> = match iter.size_hint() {
            (_, Some(n)) => Vec::with_capacity(n),
            _ => Vec::new(),
        };

        for entry in iter {
            let path = entry.unwrap().path();
            if path.is_dir() {
                modules.push(Module::new(path))
            }
        }

        Ok(modules)
    }

    pub fn install(&self, remove_existing: bool) -> Result<(), ModuleError> {
        // Check for existing system files and cleanup if required
        for system_file in self.definition.resources.values() {
            let system_file = dirs::home_dir().unwrap().join(system_file);
            if system_file.exists() && remove_existing {
                if system_file.is_file() {
                    fs::remove_file(system_file).map_err(|err| self.wrap_io_error(err))?;
                } else {
                    fs::remove_dir(system_file).map_err(|err| self.wrap_io_error(err))?;
                }
            } else if system_file.exists() {
                return Err(ModuleError::Install(
                    self.name().to_owned(),
                    system_file.display().to_string(),
                ));
            } else {
                match Module::verify_module_creation(&system_file) {
                    Err(path) => {
                        if remove_existing {
                            fs::remove_file(path).map_err(|err| self.wrap_io_error(err))?;
                        } else {
                            return Err(ModuleError::InstallPath(
                                self.name().to_string(),
                                path.display().to_string(),
                            ));
                        }
                    }
                    _ => continue,
                }
            }
        }

        // Iterate over resources and symlink them
        for (resource, system_file) in &self.definition.resources {
            let system_file = dirs::home_dir().unwrap().join(system_file);
            let resource = self.path.join(resource);
            fs::create_dir_all(system_file.parent().unwrap())
                .map_err(|err| self.wrap_io_error(err))?; // Safe as home_dir is not /
            info!(
                "Module {}: Symlink {} -> {}",
                self.name(),
                resource.display().to_string(),
                system_file.display().to_string()
            );
            symlink(&resource, &system_file).map_err(|err| self.wrap_io_error(err))?;
        }

        // Init Script
        if self.definition.init {
            let s = &self.path.clone().join(INIT_SCRIPT).display().to_string();

            info!("Module {}: Execute init script", self.name());

            let status = &process::Command::new(&s)
                .spawn()
                .map_err(|err| self.wrap_io_error(err))?
                .wait()
                .map_err(|err| self.wrap_io_error(err))?;

            if !status.success() {
                return Err(ModuleError::Exec(
                    self.name().to_owned(),
                    INIT_SCRIPT.to_owned(),
                ));
            }
        }

        Ok(())
    }

    pub fn uninstall(&self, force: bool) -> Result<(), ModuleError> {
        // Test files to verify installation
        if !force {
            for (resource, system_file) in &self.definition.resources {
                let system_file = dirs::home_dir().unwrap().join(system_file);
                let resource = self.path.join(resource);
                if !system_file.exists() {
                    continue;
                }
                match fs::read_link(system_file) {
                    Ok(actual_path) => if actual_path != resource {
                        return Err(ModuleError::Uninstall(
                            self.name().to_owned(),
                            actual_path.to_str().unwrap().to_owned(),
                        ));
                    },
                    Err(err) => return Err(ModuleError::IO(self.name().to_owned(), err)),
                }
            }
        }

        for system_file in self.definition.resources.values() {
            let system_file = dirs::home_dir().unwrap().join(system_file);

            if !system_file.exists() {
                continue;
            }

            info!(
                "Module {}: Remove {}",
                self.name(),
                system_file.display().to_string()
            );

            if system_file.is_file() {
                fs::remove_file(system_file).map_err(|err| self.wrap_io_error(err))?;
            } else {
                fs::remove_dir(system_file).map_err(|err| self.wrap_io_error(err))?;
            }
        }

        // Cleanup Script
        if self.definition.cleanup {
            let s = &self.path.clone().join(CLEANUP_SCRIPT).display().to_string();

            info!("Module {}: Execute cleanup script", self.name());

            let status = &process::Command::new(&s)
                .spawn()
                .map_err(|err| self.wrap_io_error(err))?
                .wait()
                .map_err(|err| self.wrap_io_error(err))?;

            if !status.success() {
                return Err(ModuleError::Exec(
                    self.name().to_owned(),
                    CLEANUP_SCRIPT.to_owned(),
                ));
            }
        }
        Ok(())
    }
}

fn file_name_to_string<P: AsRef<Path>>(path: P) -> String {
    path.as_ref()
        .file_name()
        .unwrap()
        .to_os_string()
        .into_string()
        .unwrap()
}

fn check_permissions(mode: u32, desired: u32) -> bool {
    ((mode >> 6) & desired) == desired
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_permissions() {
        let rwx_mode = 0x1C0;
        let rw_mode = 0x180;

        let rwx = 0b111;
        let rw = 0b110;
        let wx = 0b011;
        assert!(check_permissions(rwx_mode, rwx)); // Test equal permissions
        assert!(!check_permissions(rw_mode, wx)); // Test incorrect permissions
        assert!(check_permissions(rwx_mode, rw)); // Test strictly less permissions
        assert!(!check_permissions(rw_mode, rwx)); // Test strictly greater permissions
    }

    mod module_def {
        use super::super::*;

        #[test]
        fn test_new() {
            assert!(
                !ModuleDef::new("tests/empty").is_err(),
                "empty is a valid module"
            );
            assert!(
                !ModuleDef::new("tests/full").is_err(),
                "full is a valid module"
            );
        }
    }
}

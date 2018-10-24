#[macro_use]
extern crate clap;
extern crate dirs;
#[macro_use]
extern crate log;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_derive;

mod module;

use self::module::{Module, ModuleError};
use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};
use log::{Level, Metadata, Record};
use std::collections::HashSet;
use std::path::Path;

static LOGGER: SimpleLogger = SimpleLogger;

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!("{}", record.args());
        }
    }

    fn flush(&self) {}
}

fn main() {
    let home = dirs::home_dir()
        .expect("HOME directory could not be determined.")
        .join(".dotfiles")
        .into_os_string();
    let app = initialise(home.to_str().unwrap());
    log::set_logger(&LOGGER).unwrap();
    match app.is_present("verbose") {
        true => log::set_max_level(log::LevelFilter::Info),
        _ => log::set_max_level(log::LevelFilter::Warn),
    }

    let err = match app.subcommand() {
        ("list", Some(sub)) => list(&sub),
        ("install", Some(sub)) => install(&sub),
        ("uninstall", Some(sub)) => uninstall(&sub),
        _ => unreachable!(),
    };

    match err {
        Err(err) => err.exit(),
        _ => (),
    }
}

fn initialise(default_dir: &str) -> ArgMatches {
    App::new("modman")
        .version(crate_version!())
        .author("Reeto C. <me@ree.to>")
        .about("Dotfiles Management System for Arch Linux")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .setting(AppSettings::VersionlessSubcommands)
        .arg(
            Arg::with_name("modules-dir")
                .short("m")
                .long("modules-dir")
                .takes_value(true)
                .global(true)
                .default_value(default_dir)
                .help("Specify the module directory"),
        ).arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .global(true)
                .help("Enable verbose output"),
        ).subcommand(
            SubCommand::with_name("list")
                .about("List installable modules")
                .arg(
                    Arg::with_name("verify")
                        .long("verify")
                        .help("List all modules with verification status"),
                ),
        ).subcommand(
            SubCommand::with_name("install")
                .about("Install modules")
                .arg(
                    Arg::with_name("all")
                        .short("a")
                        .long("all")
                        .help("Install all modules"),
                ).arg(
                    Arg::with_name("force")
                        .short("f")
                        .long("force")
                        .help("Delete existing system files"),
                ).arg(
                    Arg::with_name("EXCLUDE")
                        .short("e")
                        .long("exclude")
                        .takes_value(true)
                        .multiple(true)
                        .requires("all")
                        .help("Modules to exclude"),
                ).arg(
                    Arg::with_name("MODULES")
                        .takes_value(true)
                        .multiple(true)
                        .required_unless_one(&["all", "EXCLUDE"]),
                ),
        ).subcommand(
            SubCommand::with_name("uninstall")
                .about("Uninstall modules")
                .arg(
                    Arg::with_name("all")
                        .short("a")
                        .long("all")
                        .help("Uninstall all modules"),
                ).arg(
                    Arg::with_name("force")
                        .short("f")
                        .long("force")
                        .help("Delete existing system files"),
                ).arg(
                    Arg::with_name("EXCLUDE")
                        .short("e")
                        .long("exclude")
                        .takes_value(true)
                        .multiple(true)
                        .requires("all")
                        .help("Modules to exclude"),
                ).arg(
                    Arg::with_name("MODULES")
                        .takes_value(true)
                        .multiple(true)
                        .required_unless_one(&["all", "EXCLUDE"]),
                ),
        ).get_matches()
}

fn wrap_module_err(err: module::ModuleError) -> clap::Error {
    clap::Error::with_description(&err.to_string(), clap::ErrorKind::InvalidValue)
}

fn list(app: &clap::ArgMatches) -> Result<(), clap::Error> {
    match Module::list(app.value_of("modules-dir").unwrap()) {
        Ok(modules) => {
            if app.is_present("verify") {
                for module in modules {
                    match module {
                        Ok(module) => println!("{} - OK", module.name()),
                        Err(err) => println!{"{}", err},
                    }
                }
            } else if app.is_present("verbose") {
                for module in modules {
                    match module {
                        Ok(module) => println!("{}", module),
                        _ => continue,
                    }
                }
            } else {
                for module in modules {
                    match module {
                        Ok(module) => println!("{}", module.name()),
                        _ => continue,
                    }
                }
            }
            Ok(())
        }
        Err(err @ ModuleError::Directory) => return Err(wrap_module_err(err)),
        _ => unreachable!(),
    }
}

fn resolve(app: &clap::ArgMatches) -> Result<std::vec::Vec<module::Module>, module::ModuleError> {
    // if all, then list otherwise build up
    let module_dir = Path::new(app.value_of("modules-dir").unwrap());
    if app.is_present("all") {
        let excluded_module_names: HashSet<&str> = match app.values_of("EXCLUDE") {
            Some(vals) => vals.collect(),
            None => HashSet::new(),
        };

        return Ok(Module::list(module_dir)?
            .into_iter()
            .filter_map(|m| m.ok())
            .filter(|m| !excluded_module_names.contains(m.name()))
            .collect());
    } else {
        let module_names = app.values_of("MODULES").unwrap();
        let mut modules: std::vec::Vec<module::Module> = match module_names.size_hint() {
            (_, Some(n)) => Vec::with_capacity(n),
            _ => Vec::new(),
        };
        for module_name in module_names {
            match Module::new(module_dir.join(module_name)) {
                Ok(module) => modules.push(module),
                Err(err) => return Err(err),
            }
        }

        return Ok(modules);
    }
}

fn install(app: &clap::ArgMatches) -> Result<(), clap::Error> {
    let modules = resolve(app).map_err(|err| wrap_module_err(err))?;
    for module in modules {
        match module.install(app.is_present("force")) {
            Ok(()) => println!("Module {}: Installed", module.name()),
            Err(err) => println!("{}", err.to_string()),
        }
    }
    Ok(())
}

fn uninstall(app: &clap::ArgMatches) -> Result<(), clap::Error> {
    let modules = resolve(app).map_err(|err| wrap_module_err(err))?;
    for module in modules {
        match module.uninstall(app.is_present("force")) {
            Ok(()) => println!("Module {}: Uninstalled", module.name()),
            Err(err) => println!("{}", err.to_string()),
        }
    }
    Ok(())
}

//! Minimal CLI over the downstream fleet (issue #9, slice 1).
//!
//! Argument parsing is hand-rolled on purpose: this binary has zero
//! dependencies, and a package manager must not pull an argument-parsing
//! framework for four subcommands. Every positional value goes through the
//! validated [`PluginName`]/[`GitRev`]/[`PluginSource`] constructors before
//! any spawn.

use std::path::PathBuf;

use bitty_plugin_manager::{Error, ErrorKind, Fleet, GitRev, PluginName, PluginSource};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn usage() -> String {
    "\
bitty-plugin-manager: candidate Bitty plugin package manager (downstream git-checkout fleet)

Usage:
  bitty-plugin-manager [--plugins-dir DIR] install <name> --from <source> --rev <rev>
  bitty-plugin-manager [--plugins-dir DIR] update <name> --rev <rev>
  bitty-plugin-manager [--plugins-dir DIR] rollback <name>
  bitty-plugin-manager [--plugins-dir DIR] status <name>
  bitty-plugin-manager --help | --version

Sources are https://host/path remotes or absolute local paths.
Plugins directory defaults to $XDG_DATA_HOME/bitty/plugins
(or ~/.local/share/bitty/plugins when XDG_DATA_HOME is unset)."
        .to_string()
}

/// `--flag value` and `--flag=value` forms for string flags.
fn take_flag(args: &[String], index: &mut usize, long: &str) -> Result<Option<String>, Error> {
    let arg = &args[*index];
    if let Some(value) = arg.strip_prefix(&format!("{long}=")) {
        return Ok(Some(value.to_string()));
    }
    if arg == long {
        *index += 1;
        if *index >= args.len() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("{long} needs a value"),
            ));
        }
        return Ok(Some(args[*index].clone()));
    }
    Ok(None)
}

fn default_plugins_dir() -> Result<PathBuf, Error> {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        if !data_home.is_empty() {
            return Ok(PathBuf::from(data_home).join("bitty/plugins"));
        }
    }
    let home = std::env::var_os("HOME").ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            "cannot locate plugins directory: set --plugins-dir, $XDG_DATA_HOME, or $HOME",
        )
    })?;
    Ok(PathBuf::from(home).join(".local/share/bitty/plugins"))
}

fn run(argv: &[String]) -> Result<String, Error> {
    let mut args = argv.to_vec();
    // Global flags may precede the subcommand.
    let mut plugins_dir: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        if let Some(dir) = take_flag(&args, &mut index, "--plugins-dir")? {
            plugins_dir = Some(PathBuf::from(dir));
        } else {
            break;
        }
        index += 1;
    }
    args.drain(..index.min(args.len()));
    let (command, rest) = args.split_first().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            "no subcommand; see --help".to_string(),
        )
    })?;
    if command == "--help" || command == "-h" {
        return Ok(usage());
    }
    if command == "--version" || command == "-V" {
        return Ok(format!("bitty-plugin-manager {VERSION}"));
    }
    let dir = match plugins_dir {
        Some(dir) => dir,
        None => default_plugins_dir()?,
    };
    let fleet = Fleet::open(dir);
    match command.as_str() {
        "install" => {
            let (name, source, rev) = install_args(rest)?;
            fleet.install(&name, &source, &rev)?;
            Ok(format!("installed {name} at {rev} from {source}"))
        }
        "update" => {
            let (name, rev) = update_args(rest)?;
            fleet.update(&name, &rev)?;
            Ok(format!("updated {name} to {rev}"))
        }
        "rollback" => {
            let name = single_name(rest, "rollback")?;
            fleet.rollback(&name)?;
            Ok(format!("rolled back {name}"))
        }
        "status" => {
            let name = single_name(rest, "status")?;
            match fleet.installed(&name)? {
                Some(installed) => {
                    let prev = installed
                        .prev
                        .as_ref()
                        .map(|r| r.as_str())
                        .unwrap_or("none");
                    Ok(format!(
                        "{name}: rev={} prev={prev} from {}",
                        installed.rev.as_str(),
                        installed.source.as_str()
                    ))
                }
                None => Ok(format!("{name}: not installed")),
            }
        }
        other => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("unknown subcommand {other}; see --help"),
        )),
    }
}

fn install_args(rest: &[String]) -> Result<(PluginName, PluginSource, GitRev), Error> {
    let mut name: Option<String> = None;
    let mut from: Option<String> = None;
    let mut rev: Option<String> = None;
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--from")? {
            from = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--rev")? {
            rev = Some(value);
        } else if rest[index].starts_with('-') {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("unexpected flag {}", rest[index]),
            ));
        } else if name.is_none() {
            name = Some(rest[index].clone());
        } else {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("unexpected argument {}", rest[index]),
            ));
        }
        index += 1;
    }
    let need = |field: Option<String>, what: &str| {
        field.ok_or_else(|| Error::new(ErrorKind::InvalidInput, format!("install needs {what}")))
    };
    Ok((
        PluginName::parse(&need(name, "<name>")?)?,
        PluginSource::parse(&need(from, "--from <source>")?)?,
        GitRev::parse(&need(rev, "--rev <rev>")?)?,
    ))
}

fn update_args(rest: &[String]) -> Result<(PluginName, GitRev), Error> {
    let mut name: Option<String> = None;
    let mut rev: Option<String> = None;
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--rev")? {
            rev = Some(value);
        } else if rest[index].starts_with('-') {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("unexpected flag {}", rest[index]),
            ));
        } else if name.is_none() {
            name = Some(rest[index].clone());
        } else {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("unexpected argument {}", rest[index]),
            ));
        }
        index += 1;
    }
    let name =
        name.ok_or_else(|| Error::new(ErrorKind::InvalidInput, "update needs <name>".to_string()))?;
    let rev = rev.ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            "update needs --rev <rev>".to_string(),
        )
    })?;
    Ok((PluginName::parse(&name)?, GitRev::parse(&rev)?))
}

fn single_name(rest: &[String], command: &str) -> Result<PluginName, Error> {
    if rest.len() != 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("{command} needs exactly one <name>"),
        ));
    }
    PluginName::parse(&rest[0])
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match run(&argv) {
        Ok(out) => println!("{out}"),
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}

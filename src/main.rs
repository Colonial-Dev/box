mod build;
mod cli;
mod podman;
mod user;

mod prelude {
    pub use color_eyre::eyre::{
        eyre,
        bail,
        Context as EyreContext,
        ContextCompat
    };
    
    pub use color_eyre::{
        Result,
        Report,
        Section,
        SectionExt,
    };

    pub use log::{
        debug,
        info,
        warn,
        error
    };
}

use prelude::*;
use build::*;
use cli::*;
use podman::*;

#[cfg(not(target_os = "linux"))]
compile_error!(
    "Box targets Linux only - compilation halted."
);

fn main() -> Result<()> {
    use clap::Parser;
    use indicatif::{ProgressBar, ProgressStyle};
    use Command::*;

    let args = Cli::parse();

    // Only use the verbosity flag if RUST_LOG isn't already set.
    if std::env::var("RUST_LOG").is_err() {
        match args.verbose {
            0 => std::env::set_var("RUST_LOG", "none"),
            1 => std::env::set_var("RUST_LOG", "info"),
            2 => std::env::set_var("RUST_LOG", "debug"),
            _ => std::env::set_var("RUST_LOG", "trace"),
        }
    }

    install_logging();

    info!("Parsed arguments:\n{args:#?}");

    ensure("podman")?;
    ensure("buildah")?;

    // Given a list of container names, this will
    // retrieve them in well-typed form and map an operation over them (complete with a progress spinner.)
    let map_set = |set: &ContainerSet, func: fn(&Container) -> Result<()>, op: &str| -> Result<_> {
        let style = format!(
            "{{spinner}} {op} {{msg:.green}}..."
        );

        let style = ProgressStyle::with_template(&style)
            .unwrap();

        let bar = ProgressBar::new_spinner()
            .with_style(style);

        bar.enable_steady_tick(
            std::time::Duration::from_millis(100)
        );
    
        match set.all {
            false => {
                for id in &set.containers {
                    bar.set_message(
                        id.to_owned()
                    );
                    existence_check(id)?;

                    func(
                        &Container::from_id(id)?
                    )?;
                }
            },
            true => {
                for ctr in Container::enumerate()? {
                    bar.set_message(
                        ctr
                            .annotation("box.name")
                            .unwrap_or("?")
                            .to_owned()
                    );

                    func(&ctr)?;
                }
            },
        };

        Ok(())
    };

    // Instantiates containers from a slice of images, with a progress spinner.
    let instantiate = |set: &[Image], replace| -> Result<_> {
        let style = ProgressStyle::with_template("{spinner} Creating {msg:.green}...")
            .unwrap();

        let bar = ProgressBar::new_spinner()
            .with_style(style);

        bar.enable_steady_tick(
            std::time::Duration::from_millis(100)
        );
        
        for image in set {
            bar.set_message(
                image
                    .annotation("box.name")
                    .unwrap_or("?")
                    .to_owned()
            );

            image.instantiate(replace)?;
        }

        Ok(())
    };

    match args.command {
        Containers  => list_containers()?,
        Definitions => list_definitions()?,
        Directory   => {
            println!(
                "{}",
                definition_directory()?.to_string_lossy()
            )
        },

        Create { name } => Definition::create(name)?,
        Edit   { name } => Definition::edit(name)?,
        Delete { name, yes } => Definition::delete(name, yes)?,

        Enter { name } => {
            existence_check(&name)?;

            let ctr = Container::from_id(&name)?;

            if !ctr.started() {
                ctr.start()?;
            }

            ctr.enter()?;
        },
        Exec { name, path, args } => {
            existence_check(&name)?;

            let ctr = Container::from_id(&name)?;

            if !ctr.started() {
                ctr.start()?;
            }

            ctr.exec(&path, &args)?;
        },
        Ephemeral { name, path, mut args } => {
            let image = Image::from_id(&name)?;

            args.insert(0, path);

            image.instantiate_ext(
                false,
                &args
            )?;
        },

        Build { defs, all, force } => build_set(&defs, all, force)?,

        Start   (set) => map_set(&set, Container::start, "Starting")?,
        Stop    (set) => map_set(&set, Container::stop, "Stopping")?,
        Restart (set) => map_set(&set, Container::restart, "Restarting")?,
        Down    (set) => map_set(&set, Container::down, "Removing")?,
        Reup    (set) => {
            map_set(&set, Container::down, "Removing")?;

            let set: Vec<_> = match set.all {
                false => {
                    let mut out = vec![];

                    for id in set.containers {
                        out.push(
                            Image::from_id(&id)?
                        )
                    }
                    
                    out
                },
                true => Image::enumerate()?
            };

            instantiate(&set, true)?;
        },
        Up { containers, all, replace } => {
            let set: Vec<_> = match all {
                false => {
                    let mut out = vec![];

                    for id in containers {
                        out.push(
                            Image::from_id(&id)?
                        )
                    }
                    
                    out
                },
                true => Image::enumerate()?
            };
            
            instantiate(&set, replace)?;
        }

        Init { shell } => match &*shell {
            "fish"  => {
                print!(
                    "{}",
                    include_str!("shell/fish.sh")
                )
            },
            "posix" => {
                print!(
                    "{}",
                    include_str!("shell/posix.sh")
                )
            },
            _       => unreachable!()
        },
        Config { operation, args } => evaluate_config(operation, args)?,
    }

    Ok(())
}

fn install_logging() {
    env_logger::init();

    color_eyre::config::HookBuilder::new()
        .panic_section("Well, this is embarassing. It appears Box has crashed!\nConsider reporting the bug at <https://github.com/Colonial-Dev/box>.")
        .display_env_section(false)
        .display_location_section(false)
        .install()
        .expect("Could not install Eyre hooks!");
}

/// Checks if a container exists, returning a well-formed error (with fuzzy-matched suggestions) if not.
fn existence_check(id: &str) -> Result<()> {
    use nucleo_matcher::{Matcher, Config};
    use nucleo_matcher::pattern::*;

    if Container::exists(id)? {
        Ok(())
    }
    else {
        let containers = Container::enumerate()
            .context("Failed to enumerate containers for fuzzy matching")?;

        let names = containers
            .iter()
            .filter_map(|c| c.annotation("box.name") );

        let mut matcher = Matcher::new(Config::DEFAULT);

        let matches = Pattern::new(
            id,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy
        ).match_list(names, &mut matcher);

        let suggestion = match matches.first() {
            Some(m) => format!("Did you mean '{}'?", m.0),
            None => "Did you make a typo?".to_string(),
        };

        let err = eyre!("Tried to operate on a container ({id}) that does not exist")
            .suggestion(suggestion);

        Err(err)
    }
}

/// Checks that a program exists on the system's PATH, returning a well-formed error if not.
fn ensure(program: &str) -> Result<()> {
    use std::io::ErrorKind;
    use std::process::Command;

    match Command::new(program)
        .arg("--version")
        .output()
    {
        Ok(out) => {
            info!(
                "Ensured {}",
                String::from_utf8_lossy(&out.stdout).trim()
            );

            Ok(())
        },
        Err(e) => {
            let err = if let ErrorKind::NotFound = e.kind() {
                eyre!("{program} not found in PATH")
                    .note("Box needs this program to run.")
                    .suggestion("Make sure it's installed and in your PATH.")
            } else {
                eyre!(e)
                    .wrap_err("Unable to determine if {program} exists")
                    .note("Box needs this program to run.")
                    .suggestion("Maybe you have permission issues?")
            };

            Err(err)
        }
    }
}

fn list_containers() -> Result<()> {
    use comfy_table::Table;
    use comfy_table::presets::NOTHING;

    let mut table = Table::new();
    let ctrs      = Container::enumerate()?;

    let rows = ctrs
        .iter()
        .map(|c| [
            c.annotation("box.name").unwrap(),
            c.image.as_str(),
            c.state.as_str()
        ]);

    table
        .load_preset(NOTHING)
        .set_header(["Name", "Image", "Status"])
        .add_rows(rows);

    println!("{table}");

    Ok(())
}

fn list_definitions() -> Result<()> {
    use comfy_table::Table;
    use comfy_table::presets::NOTHING;

    let mut table = Table::new();
    let defs      = Definition::enumerate()?;

    let rows = defs
        .iter()
        .map(|d| [
            d.name(),
            if d.bang.contains("fish") {
                "Fish script"
            }
            else {
                "POSIX script"
            }
        ]);
    
    table
        .load_preset(NOTHING)
        .set_header(["Name", "Type"])
        .add_rows(rows);

    println!("{table}");

    Ok(())
}

fn evaluate_config(operation: String, args: Vec<String>) -> Result<()> {
    use std::process::Command;

    let Ok(ctr) = std::env::var("__BOX_BUILD_CTR") else {
        let err = eyre!("Config command must be invoked inside of a build context")
            .suggestion("This is probably happening due to an issue with a FROM directive.")
            .suggestion("Alternately, it may be a bug in Box.");

        return Err(err);
    };
    
    // Certain operations, like ADD and RUN, need to be split
    // based on the presence of an '--' arg so they can be re-arranged
    // appropriately.
    let (args, trailing) = match args.iter().position(|i| i == "--") {
        Some(idx) => {
            let (l, r) = args.split_at(idx);

            let r = match r.len() {
                0 | 1 => [].as_slice(),
                _ => &r[1..]
            };

            (l, r)
        }
        None => (
            args.as_slice(),
            [].as_slice()
        )
    };

    debug!("Post-processed arguments: {args:?} // {trailing:?}");

    match operation.as_str() {
        // We handle ADD/COPY and RUN in Rust code,
        // because correctly handling arguments split by --
        // in shell is... non trivial.
        "run" => {
            let mut c = Command::new("buildah");

            c.arg("run");

            if trailing.is_empty() {
                c
                    .arg(ctr)
                    .arg("--")
                    .args(args);
            }
            else {
                c
                    .args(args)
                    .arg(ctr)
                    .arg("--")
                    .args(trailing);
            }

            c.spawn_ok()?
        }
        "add" => {
            let mut c = Command::new("buildah");

            c.arg("add");

            // This shuffling might seem a bit opaque,
            // but it simply exists to handle two possible cases:
            // - '--' in input; user is providing additional args to the underlying buildah invocation
            // - Inverse of above.
            if trailing.is_empty() {
                c
                    .arg(ctr)
                    .args(args);
            }
            else {
                c
                    .args(args)
                    .arg(ctr)
                    .args(trailing);
            }

            c.spawn_ok()?
        },
        "commit" => {
            let mut c = Command::new("buildah");

            c.arg("commit");

            if trailing.is_empty() {
                c
                    .arg(ctr)
                    .args(args);
            }
            else {
                c
                    .args(args)
                    .arg(ctr)
                    .args(trailing);
            }

            c.spawn_ok()?
        },
        "preset" => {
            evaluate_preset(&ctr, args)?
        },
        o if ANNOTATIONS.contains(&o) => {
            if args.is_empty() {
                bail!("Configuration value not specified")
            }

            // Gracefully handle the case where multiple values are provided.
            for a in args {
                push_annotation(
                    &ctr,
                    &format!("box.{o}"),
                    a
                )?;
            }
        },
        _ => {
            let err = eyre!("Unknown configuration option {operation}")
                .suggestion("Did you make a typo?");

            return Err(err);
        }
    }

    Ok(())
}

/// Evaluates a definition script "preset."
fn evaluate_preset(ctr: &str, args: &[String]) -> Result<()> {
    use std::ffi::OsString;
    use std::process::Command;

    let push_annotation = |key: &str, data: &str| {
        push_annotation(ctr, key, data)
    };

    let run = |cmd: &str| {
        Command::new("buildah")
            .arg("run")
            .arg(ctr)
            .arg("sh")
            .arg("-c")
            .arg(cmd)
            .spawn_ok()
            .context("Fault when running command inside working container")
    };

    let Some(name) = args.first() else {
        let err = eyre!("Preset not specified")
            .suggestion("PRESET directives cannot stand on their own");

        return Err(err)
    };

    match name.as_str() {
        "cp-user" => {
            let name = match args.get(1) {
                Some(name) => OsString::from(name),
                None => user::current_username()
                    .expect("Current user should exist")
            };

            let (name, uid, gid, shl) = user::by_name(&name)
                .expect("Current user should exist");
            
            // If your username isn't Unicode... honestly,
            // I'll be impressed enough to correct this to do proper OsString fiddling.
            let name = name.to_string_lossy();
            let shl  = shl.to_string_lossy();

            let scriptlet = format!(
                "
                set -eu
                groupadd -g {gid} {name}
                useradd -u {uid} -g {gid} -m {name} -s {shl}
                mkdir -p /etc/sudoers.d
                echo {name} ALL=\\(root\\) NOPASSWD:ALL > /etc/sudoers.d/{name}
                chmod 0440 /etc/sudoers.d/{name}
                "
            );

            run(&scriptlet)?;
        }
        "ssh-agent" => {
            let sock = std::env::var("SSH_AUTH_SOCK")
                .context("Could not fetch value of SSH_AUTH_SOCK")
                .suggestion("Is it set? Some distributions may not run an SSH agent in multi-user mode.")?;

            push_annotation(
                "box.mount",
                &format!("type=bind,src={sock},dst={sock}")
            )?;

            Command::new("buildah")
                .arg("config")
                .arg("--env")
                .arg(
                    format!("SSH_AUTH_SOCK={sock}")
                )
                .arg(ctr)
                .spawn_ok()
                .context("Fault when saving SSH_AUTH_SOCK value to container")?;
        },
        "devices" => {
            warn!("Using 'devices' preset - this will create a privileged container!");

            push_annotation("box.args", "--privileged")?;
            push_annotation("box.mount", "type=devpts,destination=/dev/pts")?;
            push_annotation("box.mount", "type=bind,src=/dev,dst=/dev,rslave=true")?;
        },
        "bind-fix" => {
            push_annotation("box.security-opt", "label=disable")?;
            push_annotation("box.userns", "keep-id")?;
        }
        _ => {
            let err = eyre!("Unrecognized preset {name}")
                .suggestion("Did you make a typo?");

            return Err(err)
        }
    }

    Ok(())
}

/// Boilerplate reduction extension trait. Wraps the `spawn` and `output` methods
/// on [`Command`](std::process::Command) to automatically generate eyre-compatible errors
/// on failure.
pub trait CommandExt {
    /// Extension method.
    /// 
    /// Wraps `output` to return either a (lossy) UTF-8 string of standard output _or_ a well-formatted error.
    fn output_ok(&mut self) -> Result<String>;

    /// Extension method.
    /// 
    /// Wraps `spawn` and `wait` to return an `Err` on non-zero exit codes without capturing the
    /// standard streams.
    fn spawn_ok(&mut self) -> Result<()>;
}

impl CommandExt for std::process::Command {
    fn output_ok(&mut self) -> Result<String> {
        debug!("Shelling out; command is {self:?}");
        
        let o = self.output()?;
    
        if o.status.success() {
            let stdout = String::from_utf8_lossy(&o.stdout)
                .to_string();
            
            Ok(stdout)
        }
        else {
            error!("Command invocation failed!");

            let arguments = format!(
                "{:?} {:?}",
                self.get_program(),
                self.get_args()
            ).header("Arguments:");
    
            let stderr = String::from_utf8_lossy(&o.stderr)
                .to_string()
                .header("Standard error:");
    
            let stdout = String::from_utf8_lossy(&o.stdout)
                .to_string()
                .header("Standard output:");
    
            let err = eyre!("command invocation failed")
                .section(arguments)
                .section(stderr)
                .section(stdout)
                .note("This is likely due to invalid input or a bug in Box.");
    
            Err(err)
        }
    }

    fn spawn_ok(&mut self) -> Result<()> {
        debug!("Shelling out; command is {self:?}");

        let make_message = |c: &std::process::Command| {
            let arguments = format!(
                "{:?} {:?}",
                c.get_program(),
                c.get_args()
            ).header("Arguments:");
    
            let err = eyre!("command invocation failed")
                .section(arguments)
                .note("This is likely due to invalid input or a bug in Box.");
    
            Err(err)
        };

        let Ok(mut child) = self.spawn() else {
            return make_message(self)
                .context("Fault when spawning command")
        };

        let Ok(status) = child.wait() else {
            return make_message(self)
                .context("Fault when running command")
        };        

        if status.success() {
            Ok(())
        }
        else {
            make_message(self)
                .context("Command returned non-zero exit code")
        }
    }
}

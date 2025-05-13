use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::prelude::*;
use crate::podman::*;
use crate::CommandExt;

pub type Definitions = Vec<Definition>;

/// Represents a Box definition.
#[derive(Debug, Hash, PartialEq, Eq)]
pub struct Definition {
    /// The path to the definition.
    pub path: PathBuf,
    /// The first line of the definition.
    pub bang: String,
    /// The hash of the definition.
    pub hash: u64,
    /// The combined hash of the definition and all of its dependencies.
    /// 
    /// Not computed by constructors; defaults to the same value as `hash`.
    pub tree: u64,
    /// Deserialized TOML metadata.
    pub meta: Metadata,
}

/// Deserialized TOML metadata from a definition.
#[derive(Debug, Hash, PartialEq, Eq, Deserialize)]
pub struct Metadata {
    /// The name of any definitions this one depends on, if any.
    #[serde(default)]
    pub depends_on    : Vec<String>,
}

impl Definition {
    /// Enumerate all definitions.
    pub fn enumerate() -> Result<Definitions> {
        use std::fs;
        use std::ffi::OsStr;

        let dir = definition_directory()?;

        let mut out = vec![];

        for entry in fs::read_dir(dir).context("Fault when starting definition enumeration")? {
            let entry = entry
                .context("Fault when iterating over definition directory")
                .suggestion("Do you have permission issues?")?;

            if entry
                .file_type()
                .context("Failed to get entry file type")?
                .is_dir() 
            {
                continue;
            }

            if entry.path().extension() == OsStr::new("box").into() {
                out.push(
                    Definition::from_path(entry.path())
                )            
            }
        }

        let (defs, errors): (Vec<_>, Vec<_>) = out
            .into_iter()
            .partition(Result::is_ok);

        if !errors.is_empty() {
            let err = errors
                .into_iter()
                .map(Result::unwrap_err)
                .fold(eyre!("Failed to load and parse definition(s)"), |acc, err| {
                    let section = format!("{err:?}")
                        .header("Sub-error:");

                    acc.section(section)
                });

            Err(err)
        }
        else {
            let defs = defs
                .into_iter()
                .map(Result::unwrap)
                .collect();

            Ok(defs)
        }
    }

    /// Given a name, attempt to find and fetch the corresponding definition.
    pub fn find(name: &str) -> Result<Self> {
        use std::fs;
        use std::ffi::OsStr;

        let dir  = definition_directory()?;
        let stem = OsStr::new(name);

        let entry = fs::read_dir(dir)
            .context("Fault when starting definition search")?
            .filter_map(Result::ok)
            .find(|e| e.path().file_stem() == stem.into());

        if let Some(entry) = entry {
            Self::from_path(entry.path())
                .context("Failed to load and parse definition")
        }
        else {
            let suggestion = match Self::alternative(name) {
                Some(m) => format!("Did you mean '{}'?", m),
                None => "Did you make a typo?".to_string(),
            };

            let err = eyre!("Tried to operate on a definition ({name}) that does not exist")
                .suggestion(suggestion);

            Err(err)
        }
    }

    // Given a name, determines whether or not a matching definition exists.
    pub fn exists(name: &str) -> Result<bool> {
        use std::fs;

        let path = definition_directory()?
            .join(
                format!("{name}.box")
            );
        
        fs::exists(path)
            .map_err(|e| {
                Report::new(e)
                    .wrap_err(
                        format!("Fault when checking if definition ({name}) exists")
                    )
            })
    }

    /// Given a path, attempts to read in its contents and parse it into a well-formed definition.
    pub fn from_path(p: impl AsRef<Path>) -> Result<Self> {
        use std::fs;

        let path = p.as_ref().to_owned();
        
        debug!("Attempting to fetch definition from path {path:?}");

        if path
            .symlink_metadata()
            .context("Fault when checking definition file matadata")
            .suggestion("Do you have permission issues?")?
            .is_symlink() && !path.exists()
        {
            let err = eyre!(
                "Definition at path {} is a broken symbolic link",
                path.to_string_lossy()
            )
            .suggestion("You're probably using some sort of dotfiles manager - is it out of sync?");

            return Err(err)
        }

        let data = fs::read_to_string(&path)
            .context(
                format!(
                    "Failed to read in definition data at path {}",
                    path.to_string_lossy()
                )
            )
            .suggestion("Do you have permission issues or non-UTF-8 data?")?;

        let bang = data 
            .lines()
            .next()
            .context("Encountered an empty definition")?
            .to_owned();

        let meta = data
            .lines()
            .filter(|l| l.starts_with("#~"))
            .fold(String::new(), |mut acc, line| {
                acc += line.trim_start_matches("#~").trim();
                acc += "\n";
                acc
            });

        let meta: Metadata = toml::from_str(&meta)
            .context("Failed to deserialize TOML frontmatter")
            .suggestion("Did you make a typo?")?;

        
        let mut hasher = DefaultHasher::new();
        
        hasher.write(
            data.as_bytes()
        );

        let hash = hasher.finish();
        
        debug!("Fetched definition from path {path:?}");

        Ok(Self { path, bang, hash, tree: hash, meta })
    }

    /// Get the name of the definition (file name minus extension.)
    pub fn name(&self) -> &str {
        use std::ffi::OsStr;

        self
            .path
            .file_stem()
            .and_then(OsStr::to_str)
            .expect("Definition name should be valid UTF-8")
    }

    /// Get the list of all definitions this one depends on.
    pub fn depends_on(&self) -> &[String] {
        &self.meta.depends_on
    }

    /// Build the definition.
    pub fn build(&self) -> Result<()> {
        use std::fs;
        use colored::Colorize;

        info!(
            "Building definition at path {:?}...",
            &self.path
        );

        eprintln!(
            "{} {}{}",
            "Building definition".bold().bright_white(),
            self.name().bold().green(),
            "...".bold().bright_white()
        );

        let script = fs::read_to_string(&self.path)
            .context("Fault when reading in definition")?;

        if !script.contains("FROM") {
            eprintln!(
                "{}{} {} {}",
                "Warning".bold().yellow(),
                ": definition".bold().bright_white(),
                self.name().bold().green(),
                "does not contain a FROM invocation".bold().bright_white()
            )
        }

        if !script.contains("COMMIT") {
            eprintln!(
                "{}{} {} {}",
                "Warning".bold().yellow(),
                ": definition".bold().bright_white(),
                self.name().bold().green(),
                "does not contain a COMMIT invocation".bold().bright_white()
            )
        }

        if self.bang.contains("fish") {
            Command::new("fish")
                .arg("-C")
                .arg("bx init fish | source")
                .arg(&self.path)
                .env(
                    "__BOX_BUILD_PATH",
                    &self.path
                )
                .env(
                    "__BOX_BUILD_DIR",
                    {
                        let mut p = self.path.to_owned();
                        p.pop();
                        p
                    }
                )
                .env(
                    "__BOX_BUILD_HASH",
                    format!("{:x}", self.hash)
                )
                .env(
                    "__BOX_BUILD_TREE",
                    format!("{:x}", self.tree)
                )
                .env(
                    "__BOX_BUILD_NAME",
                    self.name()
                )
                .spawn_ok()
                .context("Fault when evaluating Fish-based definition")?;
        }
        else {
            let script = format!(
                "source <(bx init posix)\n(\n{script}\n)",
            );
            
            let shell = self
                .bang
                .trim_start_matches("#!")
                // Whitespace after the shebang is valid.
                .trim();

            if shell.is_empty() {
                let err = eyre!("Shebang {} is invalid", &self.bang)
                    .note( "Box could not determine the interpreter path.")
                    .suggestion("Did you make a typo or forget a shebang?");

                return Err(err)
            };

            Command::new(shell)
                .arg("-c")
                .arg(script)
                .env(
                    "__BOX_BUILD_PATH",
                    &self.path
                )
                .env(
                    "__BOX_BUILD_DIR",
                    {
                        let mut p = self.path.to_owned();
                        p.pop();
                        p
                    }
                )
                .env(
                    "__BOX_BUILD_HASH",
                    format!("{:x}", self.hash)
                )
                .env(
                    "__BOX_BUILD_TREE",
                    format!("{:x}", self.tree)
                )
                .env(
                    "__BOX_BUILD_NAME",
                    self.name()
                )
                .spawn_ok()
                .context("Fault when evaluating POSIX-based definition")?;
        }
        
        Ok(())
    }

    /// Finds an alternative definition name that is similar to the given name.
    ///
    /// This function uses fuzzy matching to find a definition name that is close to the given name.
    /// If no match is found, it returns `None`.
    pub fn alternative(name: &str) -> Option<String> {
        use std::ffi::OsStr;

        use nucleo_matcher::{Matcher, Config};
        use nucleo_matcher::pattern::*;

        let defs = match Self::enumerate() {
            Ok(defs) => defs,
            Err(err) => {
                warn!("Failed to enumerate definitions for fuzzy matching: {}", err);
                return None;
            }
        };

        let names: Vec<_> = defs
            .iter()
            .filter_map(|d| {
                d
                    .path
                    .file_stem()
                    .and_then(OsStr::to_str)
            })
            .collect();

        let mut matcher = Matcher::new(Config::DEFAULT);

        Pattern::new(
            name,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy
        )
        .match_list(names, &mut matcher)
        .first()
        .map(|(m, _)| m)
        .copied()
        .map(str::to_owned)
    }
}

impl Definition {
    /// Create a new definition file with the provided name.
    pub fn create(name: String) -> Result<()> {
        use std::fs::File;
        use dialoguer::Editor;

        if Self::exists(&name)? {
            let err = eyre!("Definition {name} already exists")
                .suggestion("You may want to edit or delete it instead.");

            return Err(err);
        }

        let path = definition_directory()?
            .join(
                format!("{name}.box")
            );

        File::create(&path)
            .context("Fault when creating definition file")?;

        if let Some(data) = Editor::new()
            .require_save(true)
            .edit("#!/bin/bash\n\n")
            .context("Fault when editing new definition")?
        {
            std::fs::write(&path, data)
                .context("Fault when writing new definition to file")?
        }
        else {
            warn!("Definition creation aborted.");

            std::fs::remove_file(&path)
                .context("Fault when removing unwanted definition file")?;

            eprintln!("Creation of definition {name} aborted.")
        }
        
        Ok(())
    }

    /// Edit the specified definition file.
    pub fn edit(name: String) -> Result<()> {
        use dialoguer::Editor;

        if !Self::exists(&name)? {
            let err = eyre!("Definition {name} does not exist")
                .suggestion(
                    format!(
                        "Box checked in {}",
                        definition_directory()?.to_string_lossy()
                    )
                )                
                .suggestion("Maybe create it first?");

            return Err(err);
        }

        let path = definition_directory()?
            .join(
                format!("{name}.box")
            );

        let data = std::fs::read_to_string(&path)
            .context("Fault when reading in definition data for editing")?;

        if let Some(data) = Editor::new()
            .require_save(true)
            .edit(&data)
            .context("Fault when editing definition")?
        {
            std::fs::write(&path, data)
                .context("Fault when writing definition to file")?
        }
        else {
            warn!("Definition edit aborted.");
            eprintln!("No changes detected.")
        }

        Ok(())
    }

    /// Delete the specified definition file.
    pub fn delete(name: String, yes: bool) -> Result<()> {
        use dialoguer::Confirm;

        if !Self::exists(&name)? {
            let err = eyre!("Definition {name} does not exist")
                .suggestion(
                    format!(
                        "Box checked in {}",
                        definition_directory()?.to_string_lossy()
                    )
                )
                .suggestion("Maybe create it first?");

            return Err(err);
        }

        let path = definition_directory()?
            .join(
                format!("{name}.box")
            );

        if !yes {
            let confirm = Confirm::new()
                .with_prompt(
                    format!("Are you sure you want to remove the definition {name:?}")
                )
                .interact()
                .context("Fault when asking for user confirmation")?;

            if !confirm {
                return Ok(())
            }
        }

        std::fs::remove_file(path)
            .context("Fault when removing definition")?;

        Ok(())
    }
}

/// Determines the directory to use for definitions.
/// 
///  Existence checks these options, in this order:
/// - `$BOX_DEFINITION_DIR`
/// - `$XDG_CONFIG_HOME/box`
/// - `$HOME/.config/box`
pub fn definition_directory() -> Result<PathBuf> {
    let options = || {
        if let Ok(dir) = std::env::var("BOX_DEFINITION_DIR") {
            return Some(
                PathBuf::from(dir)
            );
        }
    
        if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
            return Some(
                PathBuf::from(xdg_config)
                    .join("box")
            );
        }
    
        if let Ok(home) = std::env::var("HOME") {
            return Some(
                PathBuf::from(home)
                    .join(".config")
                    .join("box")
            );
        }

        None
    };

    match options() {
        Some(dir) => {
            if !dir.exists() {
                std::fs::create_dir_all(&dir)
                    .context("Failed to create definition directory")?;
            }
            
            Ok(dir)
        },
        None => {
            let err = eyre!("Could not find a valid directory for definitions")
                .note("Box needs a place to store container definitions.")
                .suggestion("You likely have something wrong with your environment; Box tries:\n\t* $BOX_DEFINITION_DIR\n\t* $XDG_CONFIG_HOME/box\n\t* $HOME/.config/box\n... in that order.");

            Err(err)
        }
    }
}

/// Given a slice of definition names, attempt to fetch and build them.
/// 
/// - Alternately, if `all` is true, this function will enumerate all definitions and attempt to build them.
/// - By default, Box skips building a definition if both it and its dependencies are unchanged; `force` overrides this behavior.
pub fn build_set(defs: &[String], all: bool, force: bool) -> Result<()> {   
    use colored::Colorize;
    
    use petgraph::Graph;
    use petgraph::algo::toposort;
    use petgraph::visit::Dfs;

    let mut set: Vec<_> = match all {
        false => {
            let (defs, errors): (Vec<_>, Vec<_>) = defs
                .iter()
                .map(String::as_ref)
                .map(Definition::find)
                .partition(Result::is_ok);
            
            if !errors.is_empty() {
                let err = errors
                    .into_iter()
                    .map(Result::unwrap_err)
                    .fold(eyre!("Failed to load and parse definition(s)"), |acc, err| {
                        let section = format!("{err:?}")
                            .header("Sub-error:");

                        acc.section(section)
                    });
    
                return Err(err)
            }
            else {
                defs
                    .into_iter()
                    .map(Result::unwrap)
                    .collect()
            }
        },
        true => Definition::enumerate()?
    };

    if set.is_empty() {
        let err = eyre!("No definitions found")
            .suggestion("Did you forget to provide the definition(s) to operate on?")
            .suggestion("Alternatively, if you meant to build all definiitions, pass the -a/--all flag.");

        return Err(err);
    }

    debug!(
        "Finished build set enumeration - got {} (all: {all})\n{set:#?}",
        set.len()
    );

    debug!("Resolving dependencies...");
    
    let mut names: HashSet<_> = set
        .iter()
        .map(Definition::name)
        .collect();

    let mut deps = vec![];

    for name in set
        .iter()
        .flat_map(Definition::depends_on)
        .map(String::as_str)
    {
        if names.contains(name) {
            continue;
        }
        
        let def = Definition::find(name)
            .context("Fault when searching for definition dependency")?;

        debug!(
            "Fetched dependency {:?}",
            def
        );

        deps.push(def);
        names.insert(name);
    }

    eprintln!(
        "Building {} definitions ({} requested, {} transitive)",
        (set.len() + deps.len()).to_string().green().bold(),
        set.len().to_string().green().bold(),
        deps.len().to_string().yellow().bold(),
    );

    set.extend(deps);

    debug!(
        "Finished fetching dependencies - now working with {}\n{set:#?}",
        set.len()
    );

    let mut indices = HashMap::new();
    let mut graph   = Graph::<Definition, ()>::new();

    for def in set {
        indices.insert(
            def.name().to_owned(),
            graph.add_node(def)
        );
    }

    for idx in graph.node_indices() {
        // Borrow check complains about an immutable borrow
        // on the graph if we don't clone the dependencies.
        #[allow(clippy::unnecessary_to_owned)]
        for dep in graph[idx].depends_on().to_vec() {
            // We (counter-intuitively, at least to me)
            // insert edges in reverse; otherwise, the final
            // topological sort is inverted.
            graph.update_edge(
                indices[&dep],
                idx,
                ()
            );
        }
    }

    debug!("Walking set graph to compute tree hashes for each definition...");

    // We reverse the graph temporarily
    // in order to make the DFS work.
    graph.reverse();

    for idx in graph.node_indices() {
        debug!("Walking from {:?}", graph[idx]);

        let mut search = Dfs::new(&graph, idx);

        while let Some(nx) = search.next(&graph) {
            debug!("{:?} -> {:?}", graph[idx], graph[nx]);

            if graph[idx].tree != graph[nx].hash {
                // While probably not cryptographically sound,
                // XORing hashes together like this is commutative.
                graph[idx].tree ^= graph[nx].hash;
            }
        }
    }

    graph.reverse();

    debug!("Topologically sorting build set...");

    let topo = toposort(&graph, None)
        .map_err(|e| eyre!{"{e:?}"})
        .context("Cycle detected in definition dependency graph")?;
        
    if force {
        for idx in topo {
            graph[idx].build()?;
        }

        debug!("Finished building definition set!");

        return Ok(());
    }

    let to_u64 = |s| u64::from_str_radix(s, 16)
        .expect("Hash annotation should be a 64-bit hexadecimal number");

    let path_hash: HashMap<_, _> = Image::enumerate()
        .context("Fault when enumerating images for change detection")?
        .iter()
        .map(|i| 
            (
                i.annotation("box.path")
                    .map(PathBuf::from)
                    .expect("Path annotation should be set"),
                (
                    i.annotation("box.hash")
                        .map(to_u64)
                        .expect("Hash annotation should be set"),
                    i.annotation("box.tree")
                        .map(to_u64)
                        .expect("Tree hash annotation should be set")
                )
            )
        )
        .collect();

    debug!("Path -> Hash mapping computed:\n{path_hash:?}");

    for idx in topo {
        let def = &graph[idx];

        debug!("Inspecting... {def:?}");

        // If no image with a corresponding path exists, build.
        let Some(hashes) = path_hash.get(&def.path) else {
            def.build()?;
            continue
        };

        debug!("Hashes: {hashes:?}");

        let (own, tree) = hashes;
        
        if *own != def.hash || *tree != def.tree {
            def.build()?;
            continue
        }

        // If we got here, the build was skipped.
        eprintln!(
            "{} {} (unchanged)",
            "Skipped definition".bright_white().bold(),
            def.name().yellow().bold(),
        )
    }

    Ok(())
}
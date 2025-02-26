<h1 align="center">Box</h1>
<h3 align="center">A script-based interactive container manager.</h3>

<p align="center">
<img src="https://img.shields.io/github/actions/workflow/status/Colonial-Dev/box/rust.yml">
<img src="https://img.shields.io/github/license/Colonial-Dev/box">
<img src="https://img.shields.io/github/stars/Colonial-Dev/box">
</p>

## Features
Easily create and manage container environments for interactive use. All host integration is strictly opt-in; you choose what (if anything) is shared with each container.

<p align="center">
    <img src=".github/demo.gif">
</p>

Take advantage of Box's custom shell-based image definition format that bundles together all the information needed to build *and* run your containers.

<p align="center">
    <img src=".github/README_A.png">
</p>

Lightweight[^1], easy to install, and works on any Linux machine with `podman`.

## Installation
Before continuing, make sure you have `podman` (and `buildah`, if not included with `podman`) installed.

### From Source (Recommended)
Build-time dependencies:
- The most recent stable [Rust toolchain](https://rustup.rs/).
- A C/C++ toolchain (such as `gcc`.)

The rest is easy - just use `cargo install`, and Box will be automatically compiled and added to your `PATH`.
```sh
cargo install --locked --git https://github.com/Colonial-Dev/box --branch master
```

The same command can be used to update Box in the future.

### MUSL Binary
Alternatively, statically-linked MUSL binaries are available in the [releases](https://github.com/Colonial-Dev/box) section. 

## Getting Started

Box requires a definition for each container you'd like to create. Definitions are shell scripts (POSIX or `fish`) that run in a special harness; this injects additional functions and wraps a few others to provide functionality not present in Containerfiles, like the ability to declare runtime arguments such as mounts.

Either type must be stored with the file extension `.box` under one of:

- `$BOX_DEFINITION_DIR`
- `$XDG_CONFIG_HOME/box`
- `$HOME/.config/box`

Box checks in that order, using the first valid directory it finds.

To create and edit a new definition, you can simply run `bx create <NAME>`. This will create the file and open it using your `$EDITOR`.

`bx edit <NAME>` can be used to alter existing definitions; both commands will use a temporary file for editing.

Definitions run in the same directory as the definition, and should look something like the below. I use Fish, but the general structure
readily translates to POSIX-compatible syntaxes.

```sh
# Create a new working container.
FROM fedora-toolbox:latest

# Set up the new container...
RUN dnf install gcc

# Commit the configured container as an image.
COMMIT toolbox
```

The harness for definitions provides several tools for setting up your container.
- All Containerfile directives like `RUN` and `ADD` are polyfilled as shell functions, and generally act the same as their real counterparts. 
  - (The most notable exception is pipes and redirections in `RUN` - you must wrap them in an `sh -c` to execute them wholly inside the working container.)
- The `CFG` and `PRESET` directives, which let you:
  - Set various build-time and runtime switches
  - Provide arbitrary additional arguments to pass to `podman run`
  - Apply several prepackaged presets (such as copying a user from the host into the container, or applying security options to fix bind mounts with SELinux)

Once you have a definition, run `bx build` to compile it into an OCI image, followed by `bx up` to create a container from the image.

You can find exhaustive documentation and examples on definitions [here](https://github.com/Colonial-Dev/box/blob/master/DEFINITIONS.md).

___

For those who would like a concrete example, this is a (annotated and trimmed down) copy of the definitions I use
for my development containers.

```sh
#!/usr/bin/env fish
# A shebang is required for Box to disambiguate between Fish and POSIX.

# Fedora Toolbox is my preferred base, but there are similar images
# available for distributions like Debian and Arch.
#
# --pull=newer updates my local copy of the fedora-toolbox image if needed.
# -v $HOME/.cache/dnf... mounts a shared, persistent DNF cache into the working container - 
# good for recouping most of the speed loss from not using Containerfiles.
FROM --pull=newer -v $HOME/.cache/dnf:/var/cache/libdnf5:z fedora-toolbox:latest

# Set up DNF opts. The 'keepcache=true' in particular is critical for efficiency.
for opt in "keepcache=True" "max_parallel_downloads=8" "fastestMirror=True"
    RUN sh -c "echo $opt >> /etc/dnf/dnf.conf"
end

# Extract Chezmoi (dotfile manager) source state path.
# Being able to do stuff like this "on the fly" is one of the advantages of using
# shell to build containers.
set chezmoi (chezmoi source-path | string split /)[5..]
set chezmoi (string join / $chezmoi)

# Install my preferred shell.
RUN dnf install -y fish
# Standard development tools.
RUN dnf group install -y development-tools
# Good to have a C/++ compiler on hand, regardless of current
# toolchain.
RUN dnf group install -y c-development

# Copy my user into the container.
PRESET cp-user
# Fix Unix and SELinux permission issues with rootless mounting of host files.
PRESET bind-fix
# Mount the SSH agent socket into the container.
PRESET ssh-agent

# Copy my managed dotfiles and the associated Chezmoi binary into the container.
ADD --chown $USER:$USER -- $HOME/$chezmoi /home/$USER/$chezmoi
ADD --chown $USER:$USER -- $HOME/.config/chezmoi/chezmoi.toml /home/$USER/.config/chezmoi/chezmoi.toml
ADD (which chezmoi) /usr/bin/chezmoi

# Bootstrap all my dotfiles.
# This would also work with e.g. GNU Stow, YADM...
RUN chezmoi apply --verbose

# Set the working user to myself...
USER    $USER
# ... and the working directory to my $HOME inside the container.
WORKDIR /home/$USER
# A dummy 'infinite command' like this keeps the container alive so processes on the host
# (e.g. VSCode) can spawn 'exec' sessions inside.
CMD     "sleep inf"

# Mount my projects directory.
CFG mount type=bind,src=$HOME/Documents/Projects,dst=/home/$USER/Projects 

# Enable Podman's built-in tiny init for process reaping.
CFG args --init

# Commit the image.
COMMIT localhost/base
```

```sh
#!/usr/bin/env fish
#~ depends_on = ["base"]
# Box is capable of computing (and following) 
# a dependency graph for your definitions via the `depends_on` metadata key.

FROM localhost/base

RUN sh -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"

# Anything set in the 'base' image, including runtime options like mounts,
# is inherited - so there isn't much to do here.
COMMIT localhost/rust
```

While Box may be branded as an "interactive" container manager, it works just as well for containerized services. This definition is all I need for my Jellyfin server, including support for AMD hardware acceleration:

```sh
#!/usr/bin/env fish

FROM jellyfin/jellyfin:latest

PRESET bind-fix

CFG device /dev/dri/renderD128 
CFG mount type=bind,src=$HOME/Executable/Jellyfin/config,dst=/config
CFG mount type=bind,src=$HOME/Executable/Jellyfin/cache,dst=/cache
CFG mount type=bind,src=$HOME/Videos/DLNA,dst=/media,ro=true

CFG args "--net=host" 
CFG args "--group-add=105" 
CFG args "--user=1000:1000"

COMMIT jellyfin
```

In testing, I've had success with everything from a Minecraft server to [Ollama](https://ollama.com) by simply adapting existing Docker instructions.

## FAQ

### "How does this compare to Toolbx or Distrobox?"
It depends! I definitely wouldn't make a strict "better or worse" call.

I used to heavily rely on Toolbx for my development environments, and I also dabbled with Distrobox. Both are excellent tools, but I have one big gripe with both: host integration.

- Toolbx automatically runs as `--privileged` with (among other things) your entire `$HOME` and `$XDG_RUNTIME_DIR` mounted into the container, and offers no way to opt-out.
- Distrobox is similar, but does offer some opt-outs. You can also choose to use an alternate `$HOME` on the host (not inside the container.)

As a Silverblue user, this tight coupling with my "pure" host always left a bad taste in my mouth. Box, by contrast, is entirely opt-in when it comes to host integrations. You get to choose precisely what (if anything) is shared.

> This is good for "soft" security against stuff like supply chain attacks; if (some day) I execute a `build.rs` that tries to hijack my session tokens or wipe my system - no big deal.

Box also requires that every container be associated with a "definition," rather than defaulting to a standard "toolbox" image for each container. These use Box's custom shell-based format to declare runtime arguments (like mounts) during build time.

> I find this particularly advantageous for ensuring a consistent environment between my desktop and my laptop. It also makes for a good "lazy man's NixOS[^2]" on my Pi-hole server.

So:
- If you don't mind the above caveats and want containerized environments that Just Work with the host, use Toolbx or Distrobox.
- If you *do* mind the above caveats and/or want some declarative-ness in your containers, give Box a try.

> This is also where the name 'Box' came from; it makes boxes without any promises about the contents. You get to decide.

### "Why use shell scripts for definitions?"

Not only is shell a familiar environment that's easily extensible by external programs like Box, it also enables you to sprinkle logic into your definitions if needed.

Consider this snippet that mounts all non-hidden `$HOME` directories into the container:

```sh
for dir in (ls -p $HOME | grep /)
  CFG mount type=bind,src=(realpath $dir),dst=/home/$USER/$dir
end
```

As far as I'm aware, doing something like this in the available declarative formats (`compose` et. al.) would be a tedious manual affair duplicated across every container that needs this behavior.

### "Why not just use Kubernetes YAML or `compose`?"
A few reasons:

1.  For Box's target use case of "bespoke interactive containers," separating the information on how to *build* the image from information on how to *run* it is [suboptimal](https://htmx.org/essays/locality-of-behaviour/).
2. Kubernetes YAML is massively overcomplicated for what I wanted to do, and the `podman` version of `compose` was somewhat buggy when I tried it.
    - I was made aware as I was finishing up Box that `docker-compose` now works "out of the box" with `podman`, so if that sounds like what you want - by all means, use that instead!
3. YAML is... [yeah](https://github.com/Colonial-Dev/satpaper/blob/b2016c63ffeafc70538fd2b02fa60d1c077fd694/.github/workflows/release.yml#L1-L3).

[^1]: Single Rust binary compiled from ~2000 lines of boring plumbing code. Red Hat and the OCI have already done all the heavy lifting here!

[^2]: My apologies to any Nix fans in the audience, but my brain is too smooth to handle it.
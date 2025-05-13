# Definition Reference

## Metadata
All definitions can contain metadata as TOML key-value pairs with a special prefix:

```
#~ key = value
```

Metadata can be placed anywhere in the file. When Box evaluates a definition, each line of metadata is extracted and concatenated into a single TOML document; any intervening lines are ignored.

Currently, the following keys are recognized:
- `depends_on` (`[string]`) - a list of definition names that this definition depends on. Defaults to empty.
- `instantiate` (`bool`, default `true`) - whether or not this definition should be directly instantiable as a container.
  - Good for "base" images that are used as a foundation for multiple specialized containers rather than being utilized directly.

## Build Laziness

By default, Box only builds new and changed definitions to maximize efficiency, especially for those on slow or data-limited connections. This logic takes into account dependency trees; if `alpha` depends on `beta` and only `beta` is changed, both `alpha` and `beta` will be rebuilt.

To override this behavior, pass the `-f`/`--force` flag to `bx build`.

## Commands

Box provides (approximate) implementations of all OCI Containerfile operations as shell functions, as well as several additional tools.

### `FROM`
> *Corresponding manual page: `buildah from`*

`FROM` creates a new working container from a specified image. A `FROM` command is _required_ to precede all other operations.

```sh
FROM fedora-toolbox:latest
```

In addition to an image name, any options from the corresponding manual page can be used:

```sh
# Check if our local copy of the image is up to date,
# pulling the newer version if not.
FROM --pull=newer fedora-toolbox:latest
```

### `RUN`
> *Corresponding manual page: `buildah run`*

`RUN` executes a command inside the working container created by `FROM`:

```sh
# Most package managers will need their 'assume yes' flag
# to be set!
RUN dnf install -y gcc
```

In addition to a command, any options from the corresponding manual page can be used. You must separate any options from the command with `--` to avoid parsing ambiguity:

```sh
# This would fail due to parsing ambiguity if the '--' was removed!
RUN -v $HOME/.cache/dnf:/var/cache/dnf:z -- dnf install -y gcc
```

`RUN`'s behavior differs slightly from its OCI Containerfile equivalent when it comes to shell syntax like pipes and redirections.

This is valid `RUN` syntax in a Containerfile, but not in a Box definition:

```dockerfile
RUN echo "foo" > bar
```

In order to disambiguate, you should wrap the entire operation in a `$SHELL -c`:

```sh
RUN sh -c "echo \"foo\" > bar"
```

### `ADD` and `COPY`
> *Corresponding manual page: `buildah add`*

`ADD` and `COPY` (which defers to `ADD`) copies the contents of a file, directory, or URL into the working container created by `FROM`:

```sh
ADD $HOME/.gitconfig /home/$USER/.gitconfig
```

In addition to path(s), any options from the corresponding manual page can be used. You must separate any options from the command with `--` to avoid parsing ambiguity:

```sh
ADD --chown $USER:$USER -- $HOME/.gitconfig /home/$USER/.gitconfig
```

### Other OCI Containerfile Operations

Besides the above, Box also polyfills most other directives from the [OCI Containerfile](https://docs.docker.com/reference/dockerfile/) (aka Dockerfile) reference, including:

- `CMD`
- `ENTRYPOINT`
- `ENV`
- `EXPOSE`
  - Note that this does *not* alter any runtime behavior - it only applies metadata. Use `CFG args -p=xxxx:yyyy` to ensure ports are actually forwarded at runtime.
- `HEALTHCHECK`
- `LABEL`
- `SHELL`
- `STOPSIGNAL`
- `USER`
- `VOLUME`
- `WORKDIR`

### `CFG`

`CFG` is a catch-all tool used to bake runtime arguments (such as mounts)[^1] into an image:

```sh
CFG <FUNCTION> [ARGS...]
```

| Name | Function | More Info |
| ---- | -------- | --------- |
| `args` | Additional arbitrary arguments to pass to `podman run`. | Self-explanatory. |
| `cap-add` | Add a Linux capability to the container. | See `man capabilities`. |
| `cap-drop` | Remove a Linux capability from the container. | See `man capabilities`. |
| `cpus` | Number of CPUs the container can utilize. | Self-explanatory. |
| `memory` | Container memory limit. | Supports `b`, `k`, `m`, and `g` as suffixes. |
| `ulimit` | Set the `ulimit` parameters for the container. | See `man ulimit`. |
| `device` | Add a host device to the container. Uses `--volume` syntax. | Self-explanatory. |
| `userns` | Set the user namespace mode for the container. | [Podman docs](https://docs.podman.io/en/stable/markdown/podman-create.1.html#userns-mode) |
| `security-opt` | Set a security option for the container. | [Podman docs](https://docs.podman.io/en/stable/markdown/podman-create.1.html#security-opt-option) |
| `mount` | Add a mount (`bind` or otherwise) to the container. Uses `--mount` syntax. | [Podman docs](https://docs.podman.io/en/stable/markdown/podman-create.1.html#mount-type-type-type-specific-option) |
| `restart` | Set the container restart policy. | [Podman docs](https://docs.podman.io/en/stable/markdown/podman-create.1.html#restart-policy)
| `secret` | Give the container access to a secret. | [Podman docs](https://docs.podman.io/en/stable/markdown/podman-create.1.html#secret-secret-opt-opt) |

### `PRESET`

`PRESET` provides several "micro scripts" to take care of common operations:

```sh
PRESET <NAME> [ARGS...]
```

| Name | Function | Arguments |
| ---- | -------- | --------- |
| `cp-user` | Copies a user from the host into the container. | Optionally takes the name of the user to copy. If one is not provided, it defaults to the user executing the program (i.e. the result of `whoami`.) |
| `bind-fix` | Fixes permission issues encountered with rootless bind mounts on SELinux systems. Disables SELinux label separation and maps the host user to the same UID inside the container. | None. |
| `ssh-agent` | Mounts and exports `SSH_AUTH_SOCK` into the container. | None. |
| `devices` | Mounts `/dev` into the container. Implies `--privileged`! | None. |

### `COMMIT`
> *Corresponding manual page: `buildah commit`*

`COMMIT` is the inverse of `FROM` - it takes the working container and commits it as an image:

```sh
COMMIT image_name
```

In addition to an image name, any options from the corresponding manual page can be used. You must separate any options from the command with `--` to avoid parsing ambiguity:

```sh
COMMIT --rm -- image_name
```

## Environment Variables
Box sets the following environment variables when evaluating a definition. These are primarily an implementation detail
and should not be considered a stable interface, but knowlege of their presence may be helpful:

- `__BOX_BUILD_CTR` - the working container name. Set when `FROM` is called.
- `__BOX_BUILD_PATH` - the path to the definition.
- `__BOX_BUILD_DIR` - the path to the *directory* containing the definition.
- `__BOX_BUILD_NAME` - the name of the definition.
- `__BOX_BUILD_HASH` - the hash of the definition.
- `__BOX_BUILD_TREE` - the (somewhat poorly named) combined hash of the definition and all its dependencies.

## Functions

The harness for `fish`-based definitions includes a function called `trap` that can be used to emulate the POSIX `set -e`:

```sh
# Wraps 'cp' to exit the script on a non-zero exit code.
trap cp
```

This is not included in the POSIX harness, which automatically applies `set -eu` to abort on non-zero exit codes or uses of unset variables.

## Additional Pointers

### Persistent Package Manager Cache

One of the big advantages of Containerfiles over `buildah`-based scripts is layer caching. Every statement in a Containerfile writes a new layer that can be cached and reused in future builds. This is *especially* useful when dealing with package manager calls, as it avoids wasting huge amounts of time and bandwidth when iterating on other details in the file.

You can alleviate this by configuring a cache directory on your host that persists between builds. The below demonstrates how to accomplish this with DNF 5, but the same logic should easily transfer to your preferred package manager.

```sh
# Mount ~/.cache/dnf on my host into the standard DNF 5 cache directory in the container.
FROM -v $HOME/.cache/dnf:/var/cache/libdnf5:z fedora-toolbox:latest
# Prevent DNF from clobbering the cache by default.
RUN sh -c "echo keepcache=True >> /etc/dnf/dnf.conf"
# Install all your packages!
```

### Desktop Access

You may want to execute graphical applications or access the system clipboard inside containers. This is actually quite simple to implement!

For Wayland:

```sh
ENV WAYLAND_DISPLAY=wayland-0
# You may want to change this if your user inside the container has a different UID.
ENV XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR
CFG mount type=bind,src=$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY,dst=$XDG_RUNTIME_DIR/wayland-0
```

For X11 - warning, untested as my setup does NOT like X:

```sh
ENV DISPLAY=$DISPLAY
CFG mount type=bind,src=/tmp/.X11-unix,dst=/tmp/.X11-unix
CFG args --ipc=host
CFG device /dev/dri
```

[^1]: If you're wondering "how the hell does it do that" - it saves them as OCI annotations that are read back at creation time. <br> [Did you know you can just use the ASCII separator characters to separate things?](https://github.com/Colonial-Dev/box/blob/0c45cfe2c51a4ff1c3f62b3f753bcfeab882a56b/src/podman.rs#L341-L352) They're right there. Nobody can stop you.

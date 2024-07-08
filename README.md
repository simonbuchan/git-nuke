# `git-nuke`

## What?

`git-nuke` is a Rust binary that intends to provide a more reliable version of `git clean -dXf` for
Windows, though it may still be useful for other platforms.

## How?

Install with `cargo install git-nuke` and run with `git nuke` with the directory to be cleaned,
using the current directory by default.

If the directory is not the git working directory root, `git-nuke` will search for the git root and
include parent `.gitignore` files as git does, but will only remove directories that are within
the provided directory unless `-a / --all` is provided.

It will also not remove files in the git index, unless `-i / --ignore-index` is provided. It's quite
likely this doesn't correctly parse all the various index versions, in particular:

- git index format 4 uses a form of path differencing that is not yet supported
- the config enabling SHA-256 checksums (`extensions.objectFormat`) is also not supported yet
  as config parsing is not yet implemented

Currently, git will not create these formats by default, so it shouldn't be much of a problem yet.

## Why?

`git clean -dXf` will clear out every ignored file in a git working directory,
returning it to a clean state. Unfortunately, it does not yet understand Windows'
directory junctions, so in node monorepos / workspaces where projects use them to
reference the local code as a dependency, `git clean -dXf` will sometimes delete the
source code!

It's also absurdly slow (at least on Windows), and can use gigabytes of memory in large
repositories, which Rust makes trivial to fix.

Since it was pretty easy to add, I also put in showing a progress indicator for removing
directories. There's probably plenty of improvements here...

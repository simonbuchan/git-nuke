# `git-nuke`

## What?

`git-nuke` is a Rust binary that provides a more reliable version of `git clean -dXf` for
Windows, though it may be useful for other platforms.

## How?

Install with `cargo install git-nuke` and run with `git nuke` in a git working
directory root, or passing that directory as an argument (running in a subdirectory
probably won't do what you want, that may be addressed in a future update if anyone
cares).

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

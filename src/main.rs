//! Walks a directory, removing every file ignored by git.
//! Should behave like `git clean -fdx` but hopefully much faster.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result};
use clap::Parser;
use ignore::gitignore::GitignoreBuilder;

mod git;

#[derive(clap::Parser)]
struct Args {
    #[clap(default_value = ".")]
    dir: PathBuf,

    #[clap(short = 'n', long)]
    dry_run: bool,

    #[clap(short, long)]
    all: bool,

    #[clap(short, long)]
    verbose: bool,

    #[clap(short, long)]
    ignore_index: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let progress = indicatif::MultiProgress::new();
    let workspace = git::Workspace::find(&args.dir).context("finding git workspace")?;

    let ctx = Context {
        args,
        workspace,
        progress,
    };

    // Annoyingly, ignore has a directory walker, but it only lists *not* ignored files,
    // so we need to basically reimplement it here but with reversed logic.
    // Punt to rayon for parallelism, instead of using crossbeam-deque like it does.
    rayon::in_place_scope(|s| {
        let mut initial_work = Work::new(&ctx);
        initial_work.run(s, &ctx);
    });

    Ok(())
}

struct Context {
    args: Args,
    workspace: git::Workspace,
    progress: indicatif::MultiProgress,
}

fn paths_between<'a>(from: &Path, to: &'a Path) -> impl Iterator<Item = PathBuf> + 'a {
    assert!(from.is_absolute());
    assert!(to.is_absolute());

    let mut path = from.to_path_buf();
    std::iter::from_fn(move || {
        if path == to {
            return None;
        }
        let component = to.strip_prefix(&path).ok()?.components().next()?;
        let result = path.clone();
        path.push(component);
        Some(result)
    })
}

struct Work {
    dir: PathBuf,
    ignore: GitignoreBuilder,
    index_paths: std::sync::Arc<HashSet<PathBuf>>,
}

impl Work {
    fn new(context: &Context) -> Self {
        let dir = if context.args.all {
            context.workspace.root_dir.clone()
        } else {
            context.args.dir.clone()
        };

        let mut ignore = GitignoreBuilder::new(&context.workspace.root_dir);
        ignore.add(context.workspace.git_dir.join("info/exclude"));
        for dir in paths_between(&context.workspace.root_dir, &dir) {
            ignore.add(dir.join(".gitignore"));
        }
        let mut index_paths = HashSet::new();

        if !context.args.ignore_index {
            for entry in context.workspace.index().expect("parsing index").entries {
                // assume paths are UTF-8
                if let Ok(path) = std::str::from_utf8(&entry.name) {
                    let path = context.workspace.root_dir.join(path);
                    index_paths.insert(path);
                }
            }
        }

        let index_paths = std::sync::Arc::new(index_paths);

        Self {
            dir,
            ignore,
            index_paths,
        }
    }

    fn run<'s>(&mut self, s: &rayon::Scope<'s>, ctx: &'s Context) {
        if let Err(e) = self.try_run(s, ctx) {
            eprintln!("{}: {}", self.dir.display(), e);
        }
    }

    fn try_run<'s>(&mut self, s: &rayon::Scope<'s>, ctx: &'s Context) -> Result<()> {
        self.ignore.add(self.dir.join(".gitignore"));
        let ignore = self.ignore.build().context("building ignore")?;

        for result in std::fs::read_dir(&self.dir).context("reading dir")? {
            let entry = result.context("reading entry")?;
            let path = entry.path();
            if self.index_paths.contains(&path) {
                continue;
            }

            let is_dir = entry.file_type().context("getting entry type")?.is_dir();
            if ignore.matched(&path, is_dir).is_ignore() {
                if ctx.args.dry_run {
                    // no progress bar for dry run
                    println!("{}", path.display());
                } else if !is_dir {
                    if ctx.args.verbose {
                        let _ = ctx.progress.println(path.display().to_string());
                    }
                    std::fs::remove_file(&path).context("removing file")?;
                } else {
                    // removing big directories is slow, so we want to show progress
                    let bar = ctx
                        .progress
                        .add(
                            indicatif::ProgressBar::new_spinner()
                                .with_style(indicatif::ProgressStyle::default_spinner()),
                        )
                        .with_message(path.display().to_string());
                    bar.enable_steady_tick(Duration::from_millis(100));

                    std::fs::remove_dir_all(&path).context("removing dir")?;

                    bar.finish_and_clear();
                    if ctx.args.verbose {
                        let _ = ctx.progress.println(path.display().to_string());
                    }
                }
            } else if is_dir {
                let mut work = Work {
                    dir: path,
                    ignore: self.ignore.clone(),
                    index_paths: self.index_paths.clone(),
                };
                s.spawn(move |s| {
                    work.run(s, ctx);
                });
            }
        }

        Ok(())
    }
}

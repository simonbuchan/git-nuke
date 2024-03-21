//! Walks a directory, removing every file ignored by git.
//! Should behave like `git clean -fdx` but hopefully much faster.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result};
use clap::Parser;
use ignore::gitignore::GitignoreBuilder;

#[derive(clap::Parser)]
struct Args {
    #[clap(default_value = ".")]
    dir: PathBuf,

    #[clap(short = 'n', long)]
    dry_run: bool,

    #[clap(short, long)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();
    let progress = indicatif::MultiProgress::new();
    let ctx = Context { args, progress };

    // Annoyingly, ignore has a directory walker, but it only lists *not* ignored files,
    // so we need to basically reimplement it here but with reversed logic.
    // Punt to rayon for parallelism, instead of using crossbeam-deque like it does.
    rayon::in_place_scope(|s| {
        let mut initial_work = Work::new(&ctx.args.dir);
        initial_work.run(s, &ctx);
    });
}

struct Context {
    args: Args,
    progress: indicatif::MultiProgress,
}

struct Work {
    dir: PathBuf,
    ignore: GitignoreBuilder,
}

impl Work {
    fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            dir: dir.as_ref().to_path_buf(),
            ignore: GitignoreBuilder::new(dir),
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
                };
                s.spawn(move |s| {
                    work.run(s, ctx);
                });
            }
        }

        Ok(())
    }
}

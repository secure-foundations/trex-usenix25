use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::OsStr,
    fmt::{Debug, Display},
    path::PathBuf,
    process::Command,
    str::FromStr,
    sync::atomic::AtomicBool,
    thread::JoinHandle,
    time::Duration,
};

use anyhow::anyhow;
use clap::{Parser, Subcommand};
use dialoguer::{
    console::{style, Term},
    theme::ColorfulTheme,
    Confirm, Input, MultiSelect, Select,
};
use indicatif::{ProgressBar, ProgressFinish, ProgressStyle};
use job::{JobFail, JobRunArgs, JobSuccess};
use once_cell::sync::Lazy;
use parse_display::{Display, FromStr};
use strum::EnumIter;
use strum::IntoEnumIterator;
use sysinfo::SystemExt;

mod cache;
mod job;
use crate::job::{Job, JobType};

const POTENTIAL_REPO_ROOTS: [&str; 2] = ["companion-repo", "trex-usenix25"];

pub(crate) static QUIT_SIGNALED: AtomicBool = AtomicBool::new(false);
pub(crate) fn quit_signaled() -> bool {
    QUIT_SIGNALED.load(std::sync::atomic::Ordering::SeqCst)
}

pub(crate) static SYSTEMD_RUN_PATH: Lazy<Option<PathBuf>> = Lazy::new(|| {
    let systemd_run: PathBuf = "/usr/bin/systemd-run".into();

    if !systemd_run.exists() {
        eprintln!("WARN: Not on a system with systemd-run, memory limits not enforced");
        return None;
    }

    if std::env::var("container").unwrap_or_default() == "podman" {
        eprintln!(
            "WARN: Automatic memory limits cannot currently be enforced inside a podman container"
        );
        return None;
    }

    match Command::new("stat")
        .args(["-fc", "%T", "/sys/fs/cgroup/"])
        .output()
    {
        Err(_) => {
            eprintln!(
                "WARN: Unsuccessful in attempting to `stat -fc %T /sys/fs/cgroup/`. \
                 Memory limits not enforced.",
            );
            return None;
        }
        Ok(out) => {
            match String::from_utf8_lossy(&out.stdout).trim() {
                "cgroup2fs" => {
                    // We require cgroups v2 for memory limits to actually work.
                }
                "tmpfs" => {
                    eprintln!(
                        "WARN: On a system using cgroups v1. \
                         Memory limits not enforced. \
                         Boot with kernel parameter systemd.unified_cgroup_hierarchy=1 \
                         to enable memory limits."
                    );
                    return None;
                }
                out => {
                    eprintln!(
                        "WARN: On a system with unknown cgroups version. \
                         `stat -fc %T /sys/fs/cgroup/` returned {out:?}. \
                         Memory limits not enforced."
                    );
                    return None;
                }
            }
        }
    };

    Some(systemd_run)
});

#[derive(Display, FromStr, PartialEq, Debug, EnumIter, Clone)]
#[display(style = "snake_case")]
enum Tool {
    TRex,
    Ghidra,
    Trivial,
}

#[allow(dead_code)]
trait Possibilities: Sized {
    fn possibilities() -> Vec<Self>;
    fn question(q: impl AsRef<str>) -> Self;
    fn question_many(q: impl AsRef<str>, default: bool) -> Vec<Self>;
}
impl<T: IntoEnumIterator + Display> Possibilities for T
where
    T: FromStr,
    T: Clone,
    T::Err: Display + Debug,
{
    fn question(q: impl AsRef<str>) -> Self {
        choose_one_from(q, &Self::possibilities())
    }

    fn possibilities() -> Vec<Self> {
        Self::iter().collect()
    }

    fn question_many(q: impl AsRef<str>, default: bool) -> Vec<Self> {
        choose_many_from(q, &Self::possibilities(), |_| default)
    }
}

fn theme() -> ColorfulTheme {
    // The default heavy-checkmark in the ColorfulTheme does not work on
    // WindowsTerminal correctly due to
    //    - https://github.com/console-rs/dialoguer/issues/264
    //    - https://github.com/microsoft/terminal/issues/15592
    //    - https://github.com/microsoft/terminal/issues/13110
    //
    //  So instead of the heavy-checkmark, we use regular checkmark
    //  which renders better.
    ColorfulTheme {
        checked_item_prefix: style("✓".to_string()).for_stderr().green(),
        unchecked_item_prefix: style("✓".to_string()).for_stderr().black(),
        ..ColorfulTheme::default()
    }
}

fn confirm(q: impl AsRef<str>, default: bool) -> bool {
    Confirm::with_theme(&theme())
        .with_prompt(q.as_ref())
        .default(default)
        .report(true)
        .interact_on(&Term::stderr())
        .unwrap()
}

fn choose_one_from<T>(q: impl AsRef<str>, choices: &[T]) -> T
where
    T: Clone + Display,
{
    let selection = Select::with_theme(&theme())
        .items(&choices)
        .default(0)
        .with_prompt(q.as_ref())
        .report(true)
        .interact_on(&Term::stderr())
        .unwrap();
    choices.iter().nth(selection).unwrap().clone()
}

fn choose_many_from<T>(
    q: impl AsRef<str>,
    choices: &[T],
    preselected: impl Fn(&T) -> bool,
) -> Vec<T>
where
    T: Clone + Display,
{
    let selection = MultiSelect::with_theme(&theme())
        .items_checked(
            &choices
                .iter()
                .map(|item| (item, preselected(item)))
                .collect::<Vec<_>>(),
        )
        .with_prompt(q.as_ref())
        .report(true)
        .interact_on(&Term::stderr())
        .unwrap();
    selection.into_iter().map(|i| choices[i].clone()).collect()
}

fn get_num_from_user(q: impl AsRef<str>, default: u64) -> u64 {
    let text = Input::with_theme(&theme())
        .with_prompt(q.as_ref())
        .default(default.to_string())
        .interact_text()
        .unwrap();
    text.parse().unwrap()
}

#[derive(Display, FromStr, PartialEq, Debug, EnumIter, Clone)]
enum Benchmark {
    #[display("basic-test")]
    BasicTest,
    #[display("coreutils")]
    Coreutils,
    #[display("spec")]
    Spec,
}
impl AsRef<OsStr> for Benchmark {
    fn as_ref(&self) -> &OsStr {
        let t = match self {
            Benchmark::BasicTest => "basic-test",
            Benchmark::Coreutils => "coreutils",
            Benchmark::Spec => "spec",
        };
        assert_eq!(t, &self.to_string());
        t.as_ref()
    }
}

impl Benchmark {
    fn all_allowed_cacheable_jobs(&self) -> Vec<JobType> {
        JobType::iter().filter(|t| t.can_cache()).collect()
    }

    fn all_allowed_jobs(&self, cached: &[JobType], only_default_enabled: bool) -> Vec<JobType> {
        JobType::iter()
            .filter(|t| !cached.contains(t))
            .filter(|t| t.run_enabled_by_default() || !only_default_enabled)
            .collect::<Vec<_>>()
    }
}

pub fn glob(g: impl AsRef<str>) -> Vec<PathBuf> {
    glob::glob(g.as_ref())
        .unwrap()
        .map(|g| g.unwrap())
        .collect()
}

struct Runner {
    print_command: bool,
    queue: VecDeque<Job>,
    ask_queue: VecDeque<Job>,
    max_parallelism: u64,
    progress_bar: ProgressBar,
    workers: Vec<JoinHandle<Result<JobSuccess, JobFail>>>,
    done: usize,
    done_via_cache: usize, // subset of `done`
    done_via_skip: usize,  // subset of `done`
    failed: usize,
    retried: usize,
    message: String,
    sysinfo_system: sysinfo::System,
    timing_stats: BTreeMap<(JobType, PathBuf), Duration>,
}

impl Runner {
    fn new(msg: impl Into<String>, max_parallelism: u64) -> Self {
        let message = msg.into();
        let progress_bar = ProgressBar::new(0)
            .with_style(
                ProgressStyle::with_template(
                    // NOTE: ETA estimates are disabled, because they are inaccurate in the presence
                    // of parallelism and caching. We may wish to re-enable them later if we can
                    // make it more accurate in such scenarios.
                    //
                    // "{spinner} [{elapsed_precise} ETA={eta}] {bar:40.cyan/blue} {pos:>3}/{len:3} {msg}",
                    "{spinner} [{elapsed_precise}] {bar:40.cyan/blue} {pos:>3}/{len:3} {msg}",
                )
                .unwrap(),
            )
            .with_message(message.clone())
            .with_finish(ProgressFinish::AndLeave);
        progress_bar.enable_steady_tick(std::time::Duration::from_millis(100));
        let print_command = if let Ok(x) = std::env::var("PRINT_JOB_COMMAND") {
            if x == "1" {
                true
            } else {
                panic!("Expected PRINT_JOB_COMMAND=1 to print job commands");
            }
        } else {
            false
        };
        Self {
            print_command,
            queue: Default::default(),
            ask_queue: Default::default(),
            max_parallelism,
            progress_bar,
            workers: Default::default(),
            done: 0,
            done_via_cache: 0,
            done_via_skip: 0,
            failed: 0,
            retried: 0,
            message,
            sysinfo_system: sysinfo::System::new(),
            timing_stats: Default::default(),
        }
    }
    fn len(&self) -> usize {
        self.queue.len() + self.ask_queue.len() + self.workers.len() + self.done + self.failed
    }
    fn enqueue_many(&mut self, jobs: impl IntoIterator<Item = Job>) {
        self.queue.extend(jobs);
        self.progress_bar.set_length(self.len() as u64);
    }
    fn enough_ram_for_new_process(&mut self) -> bool {
        self.sysinfo_system.refresh_memory();
        let total = self.sysinfo_system.total_memory();
        let available = self.sysinfo_system.available_memory();
        if available > total / 2 {
            // At least half of RAM is available, ok to spin up new process
            true
        } else {
            if self.workers.is_empty() {
                eprintln!(
                    "WARNING: Not seeing half of RAM being available to spin up a new process, \
                     but there are no workers running, so spinning up a new worker to roll the dice.",
                );
                true
            } else {
                false
            }
        }
    }
    #[must_use]
    fn run_one(job: Job) -> bool {
        let mut this = Self::new(job.typ.to_string(), 1);
        this.progress_bar.finish_and_clear();
        this.print_command = true;
        this.enqueue_many([job]);
        while this.do_some_work() {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if this.done_via_cache > 0 {
            println!("To prevent reading the cache when doing a single job, use --skip-cache-read");
        }
        this.done == 1
    }
    #[must_use]
    // returns false when completely done, and no more work remains
    fn do_some_work(&mut self) -> bool {
        let quitting = quit_signaled();
        for worker in std::mem::take(&mut self.workers) {
            if worker.is_finished() {
                match worker.join().unwrap() {
                    Ok(success) => {
                        self.progress_bar.inc(1);
                        self.done += 1;
                        match success {
                            JobSuccess::ViaRun {
                                job_type,
                                base,
                                runtime,
                            } => {
                                if let Some(runtime) = runtime {
                                    self.timing_stats.insert((job_type, base), runtime);
                                }
                            }
                            JobSuccess::Skipped => {
                                self.done_via_skip += 1;
                            }
                            JobSuccess::ViaCache {
                                job_type,
                                base,
                                runtime,
                            } => {
                                if let Some(runtime) = runtime {
                                    self.timing_stats.insert((job_type, base), runtime);
                                } else {
                                    eprintln!(
                                        "WARN: No timing information available for cached `{}` / `{}`",
                                        job_type,
                                        base.display(),
                                    );
                                }
                                self.done_via_cache += 1;
                            }
                        }
                    }
                    Err(JobFail { job, reason }) => {
                        if matches!(reason, job::JobFailReason::RetryRequested) {
                            self.queue.push_back(job);
                            self.retried += 1;
                        } else {
                            self.progress_bar.println(format!(
                                "Failed because {reason}. Try again with        cargo run --bin runner -- {}",
                                job.re_runnable_command_line_flags()
                            ));
                            self.failed += 1;
                        }
                    }
                }
            } else {
                self.workers.push(worker);
            }
        }
        while self.workers.len() < self.max_parallelism as usize
            && !self.queue.is_empty()
            && self.enough_ram_for_new_process()
            && !quitting
        {
            let job = self.queue.pop_front().unwrap();
            let print_command: bool = self.print_command;
            let job_run_args = JobRunArgs {
                print_command,
                force_run_even_if_skipped: print_command,
            };
            let run_would_be_instant = job.run_would_be_instant(job_run_args);
            let worker = std::thread::spawn(move || job.run(job_run_args));
            self.workers.push(worker);
            if !run_would_be_instant {
                // We spin up only one job, rather than spinning up many jobs at once, just to
                // provide some time for processes to actually spin up a bit, which should hopefully
                // give us a better indication of memory usage (otherwise we might spin up too many
                // processes at once).
                break;
            }
        }
        if quitting && !self.workers.is_empty() {
            // Give the threads a chance to see that they've been asked to quit.
            return true;
        }
        if (self.workers.is_empty() && self.queue.is_empty()) || quitting {
            let mut message = self.message.clone();
            if self.done_via_cache > 0 {
                message += &format!(" (via cache = {})", self.done_via_cache);
            }
            if self.done_via_skip > 0 {
                message += &format!(" (via skip = {})", self.done_via_skip);
            }
            if self.retried > 0 {
                message += &format!(" (retried = {})", self.retried);
            }
            if self.failed > 0 {
                message += &format!(" (failed = {})", self.failed);
            }
            self.progress_bar.set_message(message);
            if self.done == self.len() {
                self.progress_bar.set_style(
                    ProgressStyle::with_template(
                        "{prefix:.green} [{elapsed_precise}] {pos:>3}/{len:3} {msg}",
                    )
                    .unwrap(),
                );
                self.progress_bar.set_prefix("✔");
                self.progress_bar.finish();
            } else {
                self.progress_bar.set_style(
                    ProgressStyle::with_template(
                        "{prefix:.yellow} [{elapsed_precise}] {pos:>3}/{len:3} {msg}",
                    )
                    .unwrap(),
                );
                self.progress_bar.set_prefix("!");
                self.progress_bar.abandon();
            }
            false
        } else {
            let mut details = vec![format!("active workers = {}", self.workers.len())];
            if self.retried > 0 {
                details.push(format!("retry = {}", self.retried));
            }
            if self.failed > 0 {
                details.push(format!("failed = {}", self.failed));
            }
            self.progress_bar
                .set_message(format!("{} ({})", self.message, details.join(", ")));
            true
        }
    }
}

/// Run various jobs
///
/// Pass no additional arguments to use the interactive interface
#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    #[clap(subcommand)]
    command: Option<ArgCommand>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ArgCommand {
    /// Run a single job
    SingleJob {
        /// Job type
        job: JobType,
        /// Base path for the job
        base: PathBuf,
        /// Disable timeout
        #[clap(long = "no-timeout")]
        no_timeout: bool,
        /// Disable memory limits
        #[clap(long = "no-mem-limit")]
        no_mem_limit: bool,
        /// Skip reading from cache, even if it exists
        #[clap(long = "skip-cache-read")]
        skip_cache_read: bool,
    },
    /// Run an entire stream of jobs for a benchmark
    JobsForBenchmark {
        /// The benchmark itself
        benchmark: Benchmark,
        /// Refresh caches for a particular job types
        #[clap(long = "cache-refresh", value_name = "JOB", hide = true)]
        cache_refresh: Vec<JobType>,
        /// Job types to run; if empty, run all applicable
        #[clap(value_name = "JOB")]
        run: Vec<JobType>,
        /// Parallelism
        #[clap(short = 'j')]
        parallelism: Option<Option<u64>>,
        /// Run only a single base file
        #[clap(long = "run-only-single-base")]
        run_only_single_base: Option<PathBuf>,
        /// Skip reading from cache, even if it exists
        #[clap(long = "skip-cache-read")]
        skip_cache_read: bool,
        /// Disable timeout
        #[clap(long = "no-timeout")]
        no_timeout: bool,
        /// Disable memory limits
        #[clap(long = "no-mem-limit")]
        no_mem_limit: bool,
    },
}

impl std::fmt::Display for ArgCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgCommand::SingleJob {
                job,
                base,
                no_timeout,
                no_mem_limit,
                skip_cache_read,
            } => {
                write!(
                    f,
                    "single-job '{}' '{}'{}{}{}",
                    job,
                    base.display(),
                    if *no_timeout { " --no-timeout" } else { "" },
                    if *no_mem_limit { " --no-mem-limit" } else { "" },
                    if *skip_cache_read {
                        " --skip-cache-read"
                    } else {
                        ""
                    },
                )
            }
            ArgCommand::JobsForBenchmark {
                benchmark,
                cache_refresh,
                run,
                parallelism,
                run_only_single_base,
                skip_cache_read,
                no_timeout,
                no_mem_limit,
            } => {
                write!(f, "jobs-for-benchmark '{benchmark}'")?;
                for jt in cache_refresh {
                    write!(f, " --cache-refresh '{jt}'")?;
                }
                if run != &benchmark.all_allowed_jobs(&cache_refresh, true) {
                    for jt in run {
                        write!(f, " '{jt}'")?;
                    }
                }
                match parallelism {
                    None => {}
                    Some(None) => write!(f, " -j")?,
                    Some(Some(p)) => write!(f, " -j{p}")?,
                }
                if *skip_cache_read {
                    write!(f, " --skip-cache-read")?;
                }
                if let Some(base) = run_only_single_base {
                    write!(f, " --run-only-single-base '{}'", base.display())?;
                }
                if *no_timeout {
                    write!(f, " --no-timeout")?;
                }
                if *no_mem_limit {
                    write!(f, " --no-mem-limit")?;
                }
                Ok(())
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    ctrlc::try_set_handler(move || {
        eprintln!("Triggered Ctrl-C. Killing threads.");
        QUIT_SIGNALED.store(true, std::sync::atomic::Ordering::SeqCst);
    })
    .expect("Setting Ctrl-C handler");

    let args = Args::parse();

    let cur_dir = std::env::current_dir().unwrap().canonicalize().unwrap();
    let root_dir = cur_dir
        .ancestors()
        .find(|&p| POTENTIAL_REPO_ROOTS.iter().any(|r| p.ends_with(r)))
        .unwrap();
    std::env::set_current_dir(root_dir).unwrap();

    let arg_command = if let Some(c) = args.command {
        c
    } else {
        let benchmark = Benchmark::question("Benchmark to run");
        let job_types_cache_refresh = if false {
            choose_many_from(
                "Jobs to refresh cache for (without running)",
                &benchmark.all_allowed_cacheable_jobs(),
                |_| false,
            )
        } else {
            vec![]
        };
        let job_types = choose_many_from(
            "Jobs to run",
            &benchmark.all_allowed_jobs(&job_types_cache_refresh, false),
            |t| t.run_enabled_by_default(),
        );

        let full_parallelism = confirm("Full parallelism?", true);
        let num_threads = if full_parallelism {
            std::thread::available_parallelism().unwrap().get() as u64
        } else {
            get_num_from_user("How much parallelism?", 1)
        };
        let skip_cache_read = confirm("Skip reading from cache (slow!)?", false);
        let no_timeout = confirm("No timeouts (dangerous!)?", false);
        let no_mem_limit = confirm("No memory limit (dangerous!)?", false);

        let command = ArgCommand::JobsForBenchmark {
            benchmark,
            cache_refresh: job_types_cache_refresh,
            run: job_types,
            parallelism: if full_parallelism {
                Some(None)
            } else if num_threads == 1 {
                None
            } else {
                Some(Some(num_threads))
            },
            run_only_single_base: None,
            skip_cache_read,
            no_timeout,
            no_mem_limit,
        };

        println!("To re-run with these settings, run:\n\n\tcargo run --bin runner --release -- {command}\n");

        command
    };

    match arg_command {
        ArgCommand::SingleJob {
            job,
            base,
            no_timeout,
            skip_cache_read,
            no_mem_limit,
        } => {
            if Runner::run_one(Job {
                typ: job,
                base,
                cache_refresh_only: false,
                no_timeout,
                no_mem_limit,
                skip_cache_read,
                retry_counter: 0,
            }) {
                Ok(())
            } else {
                Err(anyhow!("Job failed"))
            }
        }
        ArgCommand::JobsForBenchmark {
            benchmark,
            cache_refresh,
            run,
            parallelism,
            run_only_single_base,
            skip_cache_read,
            no_timeout,
            no_mem_limit,
        } => {
            Lazy::force(&SYSTEMD_RUN_PATH);
            if run_only_single_base.is_some() {
                assert!(cache_refresh.is_empty());
                assert!(run.is_empty());
            }
            let run = if run.is_empty() {
                benchmark.all_allowed_jobs(&cache_refresh, true)
            } else {
                run
            };
            let base_dir: PathBuf = format!("./benchmarks/{benchmark}/evalfiles/").into();
            let job_types_cache_refresh = cache_refresh;
            let mut job_types = run;
            let num_threads = match parallelism {
                None => 1,
                Some(None) => std::thread::available_parallelism().unwrap().get() as u64,
                Some(Some(p)) => p,
            };

            for &job_typ in &job_types_cache_refresh {
                if !job_typ.can_cache() {
                    println!(
                        "Cannot cache '{job_typ}', skipping refresh, and adding to list of re-runs"
                    );
                    job_types.push(job_typ);
                    continue;
                }
                let mut runner = Runner::new(format!("Cache Refresh: {job_typ}"), num_threads);
                runner.enqueue_many(job_typ.jobs_at(&base_dir, true));
                while runner.do_some_work() {
                    // runner is working, can just idle here for a short bit
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                // timing stats are not used from a pure cache refresh
            }

            let mut any_failures = false;

            let mut timing_stats: BTreeMap<(JobType, PathBuf), Duration> = Default::default();

            for &job_typ in &job_types {
                if job_types_cache_refresh.contains(&job_typ) && job_typ.can_cache() {
                    println!("Skipping '{job_typ}' because cache refreshed it");
                    continue;
                }
                let num_threads =
                    num_threads.min(job_typ.max_parallel_with_others_of_same_jobtype());
                let mut runner = Runner::new(job_typ.to_string(), num_threads);
                let jobs = match (
                    job_typ.jobs_at(&base_dir, false),
                    run_only_single_base.as_ref(),
                ) {
                    (jobs, None) => jobs,
                    (jobs, Some(base)) => {
                        let bases: BTreeSet<_> = jobs.iter().map(|job| job.base.clone()).collect();
                        if bases.contains(base) {
                            jobs.into_iter().filter(|job| &job.base == base).collect()
                        } else {
                            let potentially_valid_ones: BTreeSet<_> = bases
                                .iter()
                                .filter(|b| {
                                    b.display()
                                        .to_string()
                                        .contains(&base.display().to_string())
                                })
                                .collect();
                            panic!(
                                "Could not find base in benchmark.  \
                                 Allowed bases: {:?}.  \
                                 Did you mean one of {:?}?",
                                bases, potentially_valid_ones,
                            );
                        }
                    }
                };
                let jobs = jobs.into_iter().map(|mut j| {
                    j.skip_cache_read = skip_cache_read;
                    j.no_timeout = no_timeout;
                    j.no_mem_limit = no_mem_limit;
                    j
                });
                runner.enqueue_many(jobs);
                while runner.do_some_work() {
                    // runner is working, can just idle here for a short bit
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                if runner.failed > 0 {
                    any_failures = true;
                }
                timing_stats.extend(runner.timing_stats);
            }

            {
                use std::io::Write;
                let mut f = std::io::BufWriter::new(
                    std::fs::File::create(format!("runner-timing-{benchmark}.csv")).unwrap(),
                );
                writeln!(f, r#""JobType","Base","Time (s)""#).unwrap();
                for ((job_type, base), duration) in timing_stats {
                    let base = base.display();
                    let duration = duration.as_secs_f64();
                    writeln!(f, r#""{job_type}","{base}",{duration}"#).unwrap();
                }
            }

            if any_failures {
                Err(anyhow!("At least some jobs failed."))
            } else {
                Ok(())
            }
        }
    }
}

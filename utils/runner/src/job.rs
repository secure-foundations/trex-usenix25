use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    fmt::Debug,
    io::BufRead,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use once_cell::sync::Lazy;
use parse_display::{Display, FromStr};
use strum::EnumIter;
use thiserror::Error;

use crate::{
    cache::{Cache, CacheEntry},
    glob, quit_signaled,
};

const RUNNER_SKIP_FILE: &str = "utils/runner/runner-skip-files";
static RUNNER_SKIPS: Lazy<HashSet<String>> = Lazy::new(|| {
    // Read all non-empty lines from RUNNER_SKIP_FILE that do not start with `#`
    let mut skips = HashSet::new();
    if let Ok(f) = std::fs::File::open(RUNNER_SKIP_FILE) {
        for line in std::io::BufReader::new(f).lines() {
            let line = line.unwrap().trim().to_string();
            if !line.is_empty() && !line.starts_with('#') {
                skips.insert(line);
            }
        }
    }
    skips
});

fn expected_resym_dir_env() {
    assert!(
        std::env::var("ENABLE_RESYM").is_ok(),
        "Expected ENABLE_RESYM=1 environment variable to run this"
    );
    if std::env::var("REMOTE_RESYM_BASE_DIR").is_ok() || std::env::var("REMOTE_SERVER").is_ok() {
        assert!(
            std::env::var("REMOTE_SERVER").is_ok()
                && std::env::var("REMOTE_RESYM_BASE_DIR").is_ok(),
            "If running in remote mode, both REMOTE_SERVER _and_ REMOTE_RESYM_BASE_DIR must be set."
        );
    } else if std::env::var("RESYM_BASE_DIR").is_ok() {
        assert!(
            std::env::var("HF_TOKEN").is_ok(),
            "Expected `HF_TOKEN` to be set (see https://huggingface.co/docs/hub/security-tokens)."
        );
    } else {
        panic!("Expected at least one of `RESYM_BASE_DIR` or `REMOTE_RESYM_BASE_DIR` to be set.")
    }
}

fn resym_enabled() -> bool {
    if std::env::var("ENABLE_RESYM").is_ok() {
        expected_resym_dir_env();
        true
    } else {
        false
    }
}

// NOTE: The order of variants in the enum must be topo-sorted
#[derive(Display, FromStr, PartialEq, Eq, Debug, EnumIter, Clone, Copy, PartialOrd, Ord)]
#[display(style = "Title case")]
pub enum JobType {
    ConfirmBasicPreRequisites,
    //
    DecompressBinary,
    StripBinary,
    LiftPCode,
    ExtractVariables,
    CollectGroundTruthTypes,
    ExtractGroundTruthStructuralTypes,
    //
    #[display("Run TRex")]
    RunTRex,
    #[display("Run Ghidra part 1")]
    RunGhidraPart1,
    #[display("Run Ghidra part 2")]
    RunGhidraPart2,
    #[display("Run Trivial Baseline")]
    RunBaselineTrivial,
    #[display("Run ReSym part 0 -  decompilation")]
    DecompilationWithVarInputs,
    #[display("Run ReSym part 1")]
    RunReSymPart1,
    #[display("Run ReSym part 2")]
    RunReSymPart2,
    #[display("Run ReSym part 3")]
    RunReSymPart3,
    #[display("Run ReSym part 4")]
    RunReSymPart4,
    #[display("Run ReSym part 5")]
    RunReSymPart5,
    #[display("Run ReSym part 6")]
    RunReSymPart6,
    //
    #[display("Score TRex")]
    ScoreTRex,
    #[display("Score Ghidra")]
    ScoreGhidra,
    #[display("Score Trivial Baseline")]
    ScoreBaselineTrivial,
    #[display("Score ReSym")]
    ScoreReSym,
    //
    #[display("Generous score TRex")]
    GenerousScoreTRex,
    #[display("Generous score Ghidra")]
    GenerousScoreGhidra,
    #[display("Generous score Trivial Baseline")]
    GenerousScoreBaselineTrivial,
    #[display("Generous Score ReSym")]
    GenerousScoreReSym,
    //
    ComputeStandardMetrics,
    //
    SummarizeAllMetrics,
}

impl JobType {
    pub fn can_cache(&self) -> bool {
        match self {
            JobType::DecompressBinary
            | JobType::StripBinary
            | JobType::LiftPCode
            | JobType::ExtractVariables
            | JobType::DecompilationWithVarInputs
            | JobType::CollectGroundTruthTypes
            | JobType::ExtractGroundTruthStructuralTypes
            | JobType::RunTRex
            | JobType::RunGhidraPart1
            | JobType::RunGhidraPart2
            | JobType::RunBaselineTrivial
            | JobType::RunReSymPart1
            | JobType::RunReSymPart2
            | JobType::RunReSymPart3
            | JobType::RunReSymPart4
            | JobType::RunReSymPart5
            | JobType::RunReSymPart6
            | JobType::ScoreTRex
            | JobType::ScoreGhidra
            | JobType::ScoreBaselineTrivial
            | JobType::ScoreReSym
            | JobType::GenerousScoreTRex
            | JobType::GenerousScoreGhidra
            | JobType::GenerousScoreBaselineTrivial
            | JobType::GenerousScoreReSym
            | JobType::ComputeStandardMetrics => true,
            JobType::ConfirmBasicPreRequisites | JobType::SummarizeAllMetrics => false,
        }
    }

    pub fn max_parallel_with_others_of_same_jobtype(&self) -> u64 {
        match self {
            // These require GPU access, and thus make the system ridiculously bad if we run them in
            // parallel _locally_. On a remote server with more compute power, it is fine to push a
            // bit further.
            JobType::RunReSymPart2 | JobType::RunReSymPart4 => {
                if std::env::var("REMOTE_SERVER").is_ok() {
                    3
                } else {
                    1
                }
            }
            // By default, everything can be parallelized to anything we want
            _ => u64::MAX,
        }
    }

    pub fn run_enabled_by_default(&self) -> bool {
        match self {
            // ReSym stuff enabled iff it is explicitly enabled
            JobType::DecompilationWithVarInputs
            | JobType::RunReSymPart1
            | JobType::RunReSymPart2
            | JobType::RunReSymPart3
            | JobType::RunReSymPart4
            | JobType::RunReSymPart5
            | JobType::RunReSymPart6
            | JobType::ScoreReSym
            | JobType::GenerousScoreReSym => resym_enabled(),
            // Rest are enabled
            JobType::ConfirmBasicPreRequisites
            | JobType::DecompressBinary
            | JobType::StripBinary
            | JobType::LiftPCode
            | JobType::ExtractVariables
            | JobType::CollectGroundTruthTypes
            | JobType::ExtractGroundTruthStructuralTypes
            | JobType::RunTRex
            | JobType::RunGhidraPart1
            | JobType::RunGhidraPart2
            | JobType::RunBaselineTrivial
            | JobType::ScoreTRex
            | JobType::ScoreGhidra
            | JobType::ScoreBaselineTrivial
            | JobType::GenerousScoreTRex
            | JobType::GenerousScoreGhidra
            | JobType::GenerousScoreBaselineTrivial
            | JobType::ComputeStandardMetrics
            | JobType::SummarizeAllMetrics => true,
        }
    }

    pub fn number_of_retries_allowed(&self) -> u32 {
        match self {
            // Most jobs should succeed/fail quite deterministically, so no retries allowed.
            JobType::ConfirmBasicPreRequisites
            | JobType::DecompressBinary
            | JobType::StripBinary
            | JobType::ExtractGroundTruthStructuralTypes
            | JobType::RunTRex
            | JobType::RunGhidraPart2
            | JobType::RunBaselineTrivial
            | JobType::RunReSymPart1
            | JobType::RunReSymPart2
            | JobType::RunReSymPart3
            | JobType::RunReSymPart4
            | JobType::RunReSymPart5
            | JobType::RunReSymPart6
            | JobType::ScoreTRex
            | JobType::ScoreGhidra
            | JobType::ScoreBaselineTrivial
            | JobType::ScoreReSym
            | JobType::GenerousScoreTRex
            | JobType::GenerousScoreGhidra
            | JobType::GenerousScoreBaselineTrivial
            | JobType::GenerousScoreReSym
            | JobType::ComputeStandardMetrics
            | JobType::SummarizeAllMetrics => 0,
            // Every once in a while Ghidra just randomly fails for no understandable reason, and
            // simply retrying will cause it to succeed; by setting this to a non-zero value, we are
            // allowing those many retries before actually considering it to be a true failure,
            // thereby making sure that we aren't accidentally marking something as failed when it
            // would've succeeded.
            JobType::LiftPCode
            | JobType::ExtractVariables
            | JobType::DecompilationWithVarInputs
            | JobType::CollectGroundTruthTypes
            | JobType::RunGhidraPart1 => 2,
        }
    }

    pub fn jobs_at(&self, base_dir: &Path, cache_refresh_only: bool) -> Vec<Job> {
        assert!(base_dir.is_dir());
        assert!(base_dir.ends_with("evalfiles"));
        match self {
            JobType::DecompressBinary
            | JobType::StripBinary
            | JobType::LiftPCode
            | JobType::ExtractVariables
            | JobType::DecompilationWithVarInputs
            | JobType::CollectGroundTruthTypes
            | JobType::ExtractGroundTruthStructuralTypes
            | JobType::RunTRex
            | JobType::RunGhidraPart1
            | JobType::RunGhidraPart2
            | JobType::RunBaselineTrivial
            | JobType::RunReSymPart1
            | JobType::RunReSymPart2
            | JobType::RunReSymPart3
            | JobType::RunReSymPart4
            | JobType::RunReSymPart5
            | JobType::RunReSymPart6
            | JobType::ScoreTRex
            | JobType::ScoreGhidra
            | JobType::ScoreBaselineTrivial
            | JobType::ScoreReSym
            | JobType::GenerousScoreTRex
            | JobType::GenerousScoreGhidra
            | JobType::GenerousScoreBaselineTrivial
            | JobType::GenerousScoreReSym
            | JobType::ComputeStandardMetrics => {
                glob(format!("{}/**/*.binar.xz", base_dir.display()))
                    .into_iter()
                    .map(|p| {
                        // twice, to remove both `.xz` and `.binar` respectively
                        p.with_extension("").with_extension("")
                    })
                    .map(|base| Job {
                        base,
                        typ: *self,
                        cache_refresh_only,
                        no_timeout: false,
                        no_mem_limit: false,
                        skip_cache_read: false,
                        retry_counter: 0,
                    })
                    .collect()
            }

            JobType::ConfirmBasicPreRequisites | JobType::SummarizeAllMetrics => vec![Job {
                base: base_dir.into(),
                typ: *self,
                cache_refresh_only,
                no_timeout: false,
                no_mem_limit: false,
                skip_cache_read: false,
                retry_counter: 0,
            }],
        }
    }
}

pub enum JobSuccess {
    ViaRun {
        job_type: JobType,
        base: PathBuf,
        runtime: Option<Duration>,
    },
    ViaCache {
        job_type: JobType,
        base: PathBuf,
        runtime: Option<Duration>,
    },
    Skipped,
}

#[derive(Error, Debug)]
#[error("job failed due to {reason}")]
pub struct JobFail {
    pub job: Job,
    pub reason: JobFailReason,
}

#[derive(Error, Debug)]
pub enum JobFailReason {
    #[error("retry requested")]
    RetryRequested,
    #[error("at least one input file not found: {0:?}")]
    InputFileNotFound(PathBuf),
    #[error("at least one output file not found {0:?}")]
    OutputFileNotFound(PathBuf),
    #[error("failed to insert into cache due to {0}")]
    CacheInsertFail(anyhow::Error),
    #[error("running the job failed")]
    JobRunFail,
}

#[derive(Clone)]
pub struct Job {
    pub base: PathBuf,
    pub typ: JobType,
    pub cache_refresh_only: bool,
    pub no_timeout: bool,
    pub no_mem_limit: bool,
    pub skip_cache_read: bool,
    // `retry_counter` must start at 0; it is only incremented if a job is sent back to be retried;
    // it should not be used for anything else.
    pub retry_counter: u32,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("typ", &self.typ)
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy)]
pub struct JobRunArgs {
    pub print_command: bool,
    pub force_run_even_if_skipped: bool,
}

impl Job {
    pub fn re_runnable_command_line_flags(&self) -> String {
        let cache_stuff = if self.cache_refresh_only {
            " # To rebuild, so that _then_ it can be cached"
        } else {
            ""
        };
        format!(
            "{}{}",
            crate::ArgCommand::SingleJob {
                job: self.typ,
                base: self.base.clone(),
                no_timeout: self.no_timeout,
                no_mem_limit: self.no_mem_limit,
                skip_cache_read: self.skip_cache_read,
            },
            cache_stuff
        )
    }

    fn inputs_dependencies_and_outputs(&self) -> (Vec<PathBuf>, Vec<&str>, Vec<PathBuf>) {
        let w = |e: &str| {
            // We do this manually, rather than via `with_extension` since the base itself might
            // have _some_ `.` in it already. Unfortunately, there isn't a nice function to say
            // "append extension", so we do it this weird way.
            PathBuf::from(self.base.clone().into_os_string().into_string().unwrap() + "." + e)
        };

        // This file has information about the version of Ghidra
        const GHIDRA: &str = "/opt/ghidra/Ghidra/application.properties";

        match self.typ {
            JobType::DecompressBinary => (vec![w("binar.xz")], vec![], vec![w("binar")]),
            JobType::StripBinary => (vec![w("binar")], vec![], vec![w("ndbg-bin")]),
            JobType::LiftPCode => (
                vec![w("ndbg-bin")],
                vec![
                    GHIDRA,
                    "./**/ghidra_headless_scripts/src/PCodeExporter.java",
                ],
                vec![w("lifted")],
            ),
            JobType::ExtractVariables => (
                vec![w("binar")],
                vec![
                    GHIDRA,
                    "./**/ghidra_headless_scripts/src/VariableExporter.java",
                ],
                vec![w("vars")],
            ),
            JobType::DecompilationWithVarInputs => (
                vec![w("ndbg-bin"), w("vars")],
                vec![
                    GHIDRA,
                    "./**/ghidra_headless_scripts/src/DecompilationDumpWithVariableInputs.java",
                ],
                vec![w("decompiled-wvi")],
            ),
            JobType::CollectGroundTruthTypes => (
                vec![w("binar")],
                vec![
                    GHIDRA,
                    "./**/ghidra_headless_scripts/src/TypesExporter.java",
                    "./**/ghidra_headless_scripts/src/TypesDump.java",
                ],
                vec![w("types")],
            ),
            JobType::ExtractGroundTruthStructuralTypes => (
                vec![w("types")],
                vec!["./trex/**/*.rs", "./utils/types2st/**/*.rs"],
                vec![w("gtst")],
            ),
            JobType::RunTRex => (
                vec![w("lifted"), w("vars")],
                vec!["./trex/**/*.rs"],
                vec![w("trex-st"), w("trex-clike"), w("trex-ssa"), w("trex-log")],
            ),
            JobType::RunGhidraPart1 => (
                vec![w("ndbg-bin"), w("vars")],
                vec![
                    GHIDRA,
                    "./**/ghidra_headless_scripts/src/TypesRecovererWithVariableInputs.java",
                    "./**/ghidra_headless_scripts/src/TypesDump.java",
                ],
                vec![w("ghidra-wvi-types")],
            ),
            JobType::RunGhidraPart2 => (
                vec![w("ghidra-wvi-types")],
                vec!["./trex/**/*.rs", "./utils/types2st/**/*.rs"],
                vec![w("ghidra-wvi-st")],
            ),
            JobType::RunReSymPart1 => (
                vec![w("decompiled-wvi")],
                vec!["./tools/evaluating_resym/convert_to_resym_vardecoder_input_format.py"],
                vec![w("resym-vardecoder-inp")],
            ),
            JobType::RunReSymPart2 => (
                vec![w("resym-vardecoder-inp")],
                vec![
                    // Path to ReSym's script and the vardecoder. Not mentioning it here is not
                    // really an issue, since that code should be static/unchanging
                ],
                vec![w("resym-vardecoder-out")],
            ),
            JobType::RunReSymPart3 => (
                vec![w("resym-vardecoder-out")],
                vec!["./tools/evaluating_resym/convert_to_resym_fielddecoder_input_format.py"],
                vec![w("resym-fielddecoder-inp")],
            ),
            JobType::RunReSymPart4 => (
                vec![w("resym-fielddecoder-inp")],
                vec![
                    // Path to ReSym's script and the vardecoder. Not mentioning it here is not
                    // really an issue, since that code should be static/unchanging
                ],
                vec![w("resym-fielddecoder-out")],
            ),
            JobType::RunReSymPart5 => (
                vec![w("resym-fielddecoder-out")],
                vec!["./tools/evaluating_resym/process_resym_output.py"],
                vec![w("resym-types")],
            ),
            JobType::RunReSymPart6 => (
                vec![w("resym-types")],
                vec!["./trex/**/*.rs", "./utils/types2st/**/*.rs"],
                vec![w("resym-st")],
            ),
            JobType::RunBaselineTrivial => (
                vec![w("vars")],
                vec!["./tools/baselinetrivial/**/*.rs"],
                vec![w("baselinetrivial-st")],
            ),
            JobType::ScoreTRex
            | JobType::ScoreGhidra
            | JobType::ScoreBaselineTrivial
            | JobType::ScoreReSym => {
                let extx = match self.typ {
                    JobType::ScoreTRex => "trex",
                    JobType::ScoreGhidra => "ghidra-wvi",
                    JobType::ScoreBaselineTrivial => "baselinetrivial",
                    JobType::ScoreReSym => "resym",
                    _ => unreachable!(),
                };
                let scorer_ext = format!("scorer-{extx}");
                let test_ext = format!("{extx}-st");
                (
                    vec![w("gtst"), w(&test_ext)],
                    vec!["./trex/**/*.rs", "./metrics/scorer/**/*.rs"],
                    vec![w(&scorer_ext)],
                )
            }
            JobType::GenerousScoreTRex
            | JobType::GenerousScoreGhidra
            | JobType::GenerousScoreBaselineTrivial
            | JobType::GenerousScoreReSym => {
                let extx = match self.typ {
                    JobType::GenerousScoreTRex => "trex",
                    JobType::GenerousScoreGhidra => "ghidra-wvi",
                    JobType::GenerousScoreBaselineTrivial => "baselinetrivial",
                    JobType::GenerousScoreReSym => "resym",
                    _ => unreachable!(),
                };
                let scorer_ext = format!("gen-scorer-{extx}");
                let test_ext = format!("{extx}-st");
                (
                    vec![w("gtst"), w(&test_ext)],
                    vec!["./trex/**/*.rs", "./metrics/scorer/**/*.rs"],
                    vec![w(&scorer_ext)],
                )
            }
            JobType::ComputeStandardMetrics => (
                [
                    w("gtst"),
                    w("baselinetrivial-st"),
                    w("ghidra-wvi-st"),
                    w("trex-st"),
                ]
                .into_iter()
                .chain(if resym_enabled() {
                    vec![w("resym-st")].into_iter()
                } else {
                    vec![].into_iter()
                })
                .collect(),
                vec![
                    "./trex/**/*.rs",
                    "./metrics/scorer/**/*.rs",
                    "./metrics/standardized-scoring/**/*.rs",
                ],
                vec![w("stdmetrics")],
            ),
            JobType::ConfirmBasicPreRequisites | JobType::SummarizeAllMetrics => {
                // uncacheable things don't need to specify inputs/outputs
                assert!(!self.typ.can_cache());
                Default::default()
            }
        }
    }

    #[must_use]
    // temp: returns if job succeeded
    fn do_job_ignoring_cache(
        &self,
        inputs: &[PathBuf],
        outputs: &[PathBuf],
        print_command: bool,
    ) -> bool {
        if quit_signaled() {
            eprintln!("Refusing to start new processes once quit has been signaled.");
            return false;
        }

        let base = &self.base;

        macro_rules! cmd {
            (! $cmd:literal) => { cmd!("just", $cmd, base) };
            ($($arg:expr),* $(,)?) => {{
                let mut command: Vec<OsString> = vec![];
                $(command.push($arg.into());)*
                command
            }};
        }

        let cmd = match self.typ {
            JobType::ConfirmBasicPreRequisites => {
                cmd!("just", "confirm-basic-pre-requisites")
            }
            JobType::DecompressBinary => {
                let xz = &inputs[0];
                cmd!("unxz", "--force", "--keep", xz)
            }
            JobType::StripBinary => {
                let inp = &inputs[0];
                let out = &outputs[0];
                cmd!("llvm-objcopy", "--strip-debug", inp, out)
            }
            JobType::LiftPCode => cmd!(!"pcode-export"),
            JobType::ExtractVariables => cmd!(!"var-extract"),
            JobType::DecompilationWithVarInputs => cmd!(!"decompilation-wvi-export"),
            JobType::CollectGroundTruthTypes => cmd!(!"type-extract"),
            JobType::ExtractGroundTruthStructuralTypes => cmd!(!"gen-struct-types"),
            JobType::RunTRex => {
                let lifted = &inputs[0];
                let vars = &inputs[1];
                let st = &outputs[0];
                let clike = &outputs[1];
                let ssa = &outputs[2];
                let log = &outputs[3];
                cmd!(
                    "cargo",
                    "run",
                    "--bin",
                    "trex",
                    "--release",
                    "--",
                    "from-ghidra",
                    lifted,
                    vars,
                    "--output-structural",
                    st,
                    "--output-c-like",
                    clike,
                    "-ddd",
                    "--dump-ssa-lifted",
                    ssa,
                    "--log",
                    log,
                )
            }
            JobType::RunGhidraPart1 => cmd!(!"run-ghidra-wvi-part1"),
            JobType::RunGhidraPart2 => {
                cmd!(!"run-ghidra-wvi-part2")
            }
            JobType::RunBaselineTrivial => {
                let vars = &inputs[0];
                let st = &outputs[0];
                cmd!(
                    "cargo",
                    "run",
                    "--bin",
                    "baselinetrivial",
                    "--release",
                    "--",
                    vars,
                    "--output",
                    st
                )
            }
            JobType::RunReSymPart1 => {
                expected_resym_dir_env();
                cmd!(!"run-resym-part1")
            }
            JobType::RunReSymPart2 => {
                expected_resym_dir_env();
                if std::env::var("REMOTE_SERVER").is_ok() {
                    cmd!(!"remote-run-resym-part2")
                } else {
                    cmd!(!"run-resym-part2")
                }
            }
            JobType::RunReSymPart3 => {
                expected_resym_dir_env();
                cmd!(!"run-resym-part3")
            }
            JobType::RunReSymPart4 => {
                expected_resym_dir_env();
                if std::env::var("REMOTE_SERVER").is_ok() {
                    cmd!(!"remote-run-resym-part4")
                } else {
                    cmd!(!"run-resym-part4")
                }
            }
            JobType::RunReSymPart5 => {
                expected_resym_dir_env();
                cmd!(!"run-resym-part5")
            }
            JobType::RunReSymPart6 => {
                expected_resym_dir_env();
                cmd!(!"run-resym-part6")
            }

            JobType::ScoreTRex
            | JobType::ScoreGhidra
            | JobType::ScoreBaselineTrivial
            | JobType::ScoreReSym => {
                let gtst = &inputs[0];
                let test = &inputs[1];
                let output = &outputs[0];
                cmd!(
                    "cargo",
                    "run",
                    "--bin",
                    "scorer",
                    "--release",
                    "--",
                    "--ground-truth",
                    gtst,
                    "--test",
                    test,
                    "--output-finer-grained-csv",
                    output,
                )
            }
            JobType::GenerousScoreTRex
            | JobType::GenerousScoreGhidra
            | JobType::GenerousScoreBaselineTrivial
            | JobType::GenerousScoreReSym => {
                let gtst = &inputs[0];
                let test = &inputs[1];
                let output = &outputs[0];
                cmd!(
                    "cargo",
                    "run",
                    "--bin",
                    "scorer",
                    "--release",
                    "--",
                    "--ground-truth",
                    gtst,
                    "--test",
                    test,
                    "--output-finer-grained-csv",
                    output,
                    "--enable-generous-eval",
                )
            }
            JobType::ComputeStandardMetrics => cmd!(!"std-metrics"),
            JobType::SummarizeAllMetrics => {
                let dir = base;
                let benchname = base.parent().unwrap().file_name().unwrap();
                let outdir = base.parent().unwrap();
                cmd!("just", "summarize-all", dir, benchname, outdir)
            }
        };

        let timeout_args = [
            "--verbose",
            // Attempt to nicely kill the process after some time; but if it hasn't died (or
            // refuses to die) after we tell it to, then forcefully kill it with brute-force
            // (i.e., SIGKILL).
            "--kill-after=2",
            if self.no_timeout {
                // If we have no timeout set, we still have the "you cannot refuse to die" code, which
                // will trigger if a SIGINT (Ctrl-C) is sent, and the process hasn't yet died after 2
                // seconds.
                "0"
            } else {
                "1800"
            },
        ];

        let mem_limit_args = if self.no_mem_limit {
            vec![]
        } else {
            // Allow only a fixed amount of memory per process group (if you do it with rlimit, then
            // it is per process, but we want it per process group)
            match &*crate::SYSTEMD_RUN_PATH {
                Some(systemd_run) => cmd!(
                    systemd_run,
                    "--quiet",
                    "--user",
                    "--scope",
                    "--property",
                    "MemoryMax=64G",
                    "--property",
                    "MemorySwapMax=0",
                )
                .into(),
                None => vec![],
            }
        };

        let mut command = Command::new("timeout");
        command.args(&timeout_args);
        command.args(&mem_limit_args);
        command.args(&cmd);
        command.stdin(Stdio::null());

        if print_command {
            println!(
                "$ timeout {} {}{}",
                shell_quote_command_args(timeout_args),
                if mem_limit_args.is_empty() {
                    "".into()
                } else {
                    shell_quote_command_args(mem_limit_args) + " "
                },
                shell_quote_command_args(cmd),
            );
            command.stdout(Stdio::inherit());
            command.stderr(Stdio::inherit());
        } else {
            command.stdout(Stdio::null());
            command.stderr(Stdio::null());
        };

        let mut child = command.spawn().expect("Spawning command");

        loop {
            if quit_signaled() {
                let child_id = child.id();
                let kill_success = Command::new("kill")
                    .arg("-s")
                    .arg("TERM")
                    .arg(child_id.to_string())
                    .status()
                    .expect("Killing child process")
                    .success();
                if !kill_success {
                    panic!("Failed to kill child process");
                }
                return false;
            }
            if let Some(exit_status) = child.try_wait().expect("Trying to wait on child") {
                return exit_status.success();
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    pub fn run_would_be_instant(&self, args: JobRunArgs) -> bool {
        if args.force_run_even_if_skipped {
            return false;
        }
        if RUNNER_SKIPS.contains(&self.base.display().to_string()) {
            // fast because skipped
            return true;
        }

        let (inp, dep, out) = self.inputs_dependencies_and_outputs();
        if !inp.iter().all(|i| i.exists()) {
            // fast because it'll complain quickly
            return true;
        }

        if self.cache_refresh_only {
            // fast because it only needs to refresh cache
            return true;
        }

        if !self.typ.can_cache() {
            return false;
        }

        if self.skip_cache_read {
            return false;
        }

        let cached = Cache::new(&self.typ).get(&inp, &dep);

        if !out.iter().all(|o| cached.contains_key(o)) {
            return false;
        } else {
            // fast because cache is sufficient, can pull from there
            return true;
        }
    }

    pub fn run(self, args: JobRunArgs) -> Result<JobSuccess, JobFail> {
        if quit_signaled() {
            eprintln!("Refusing to start new jobs once quit has been signaled.");
            return Err(JobFail {
                job: self,
                reason: JobFailReason::JobRunFail,
            });
        }

        assert!(crate::POTENTIAL_REPO_ROOTS
            .iter()
            .any(|r| std::env::current_dir().unwrap().ends_with(r)));
        assert!(self.base.is_relative());

        if RUNNER_SKIPS.contains(&self.base.display().to_string()) {
            if args.force_run_even_if_skipped {
                eprintln!("Forcing run (for what would otherwise be skipped) for {self:?}");
            } else {
                return Ok(JobSuccess::Skipped);
            }
        }

        let (inp, dep, out) = self.inputs_dependencies_and_outputs();

        for i in &inp {
            if !i.exists() {
                return Err(JobFail {
                    job: self,
                    reason: JobFailReason::InputFileNotFound(i.clone()),
                });
            }
        }

        let mut cache = Cache::new(&self.typ);

        if self.cache_refresh_only {
            assert!(self.typ.can_cache());
            for o in &out {
                if !o.exists() {
                    return Err(JobFail {
                        job: self,
                        reason: JobFailReason::OutputFileNotFound(o.clone()),
                    });
                }
            }
            match cache.insert(&inp, &dep, &out, None) {
                Ok(()) => {
                    return Ok(JobSuccess::ViaRun {
                        job_type: self.typ,
                        base: self.base,
                        runtime: None,
                    });
                }
                Err(e) => {
                    return Err(JobFail {
                        job: self,
                        reason: JobFailReason::CacheInsertFail(e),
                    });
                }
            }
        }

        let cached = if self.typ.can_cache() {
            cache.get(&inp, &dep)
        } else {
            Default::default()
        };

        if self.skip_cache_read
            || !self.typ.can_cache()
            || !out.iter().all(|o| cached.contains_key(o))
        {
            // cache is insufficient
            let start_time = std::time::Instant::now();
            if self.do_job_ignoring_cache(&inp, &out, args.print_command) {
                let elapsed_time = start_time.elapsed();
                if self.typ.can_cache() {
                    match cache.insert(&inp, &dep, &out, Some(elapsed_time)) {
                        Ok(()) => {}
                        Err(e) => {
                            return Err(JobFail {
                                job: self,
                                reason: JobFailReason::CacheInsertFail(e),
                            });
                        }
                    }
                }
                return Ok(JobSuccess::ViaRun {
                    job_type: self.typ,
                    base: self.base,
                    runtime: Some(elapsed_time),
                });
            } else {
                // delete all files in `out`, since the job failed
                let mut dirs_to_remove = vec![];
                for o in out {
                    if o.exists() {
                        if o.is_dir() {
                            dirs_to_remove.push(o);
                        } else {
                            std::fs::remove_file(&o).unwrap();
                        }
                    }
                }

                // delete directories in `out` that are empty; recursively until there are no such
                // empty directories
                loop {
                    let old_dirs_to_remove = dirs_to_remove.clone();

                    for dir in std::mem::take(&mut dirs_to_remove) {
                        assert!(dir.exists());
                        if dir.read_dir().unwrap().next().is_none() {
                            std::fs::remove_dir(&dir).unwrap();
                        } else {
                            dirs_to_remove.push(dir);
                        }
                    }

                    if old_dirs_to_remove == dirs_to_remove {
                        break;
                    }
                }

                if !dirs_to_remove.is_empty() {
                    eprintln!(
                        "The following directories were not empty and were not deleted: {:?}",
                        dirs_to_remove
                    );
                }

                if self.retry_counter == self.typ.number_of_retries_allowed() {
                    return Err(JobFail {
                        job: self,
                        reason: JobFailReason::JobRunFail,
                    });
                } else {
                    return Err(JobFail {
                        job: Job {
                            retry_counter: self.retry_counter + 1,
                            ..self
                        },
                        reason: JobFailReason::RetryRequested,
                    });
                }
            }
        } else {
            // cache is enough, pull from there
            for o in out {
                match &cached[&o] {
                    CacheEntry::Dir => {
                        std::fs::create_dir_all(o).unwrap();
                    }
                    CacheEntry::File(cache_path) => {
                        if o.exists() {
                            assert!(!o.is_dir());
                            std::fs::remove_file(&o).unwrap();
                        }
                        std::fs::copy(cache_path, &o).unwrap();
                    }
                }
            }
            return Ok(JobSuccess::ViaCache {
                job_type: self.typ,
                base: self.base,
                runtime: Cache::get_runtime(&cached),
            });
        }
    }
}

fn shell_quote(s: &OsStr) -> String {
    let s = s.to_str().expect("Valid UTF-8");
    if !s.chars().all(|c| match c {
        ' ' => {
            // Needs special handling, but is now handled
            true
        }
        '/' | '.' | '-' | '=' | '_' | '[' | ']' => {
            // No special handling needed
            true
        }
        _ if c.is_ascii_alphanumeric() => {
            // No special handling needed
            true
        }
        _ => {
            eprintln!("Currently unhandled character {c:?} in shell_quote routine");
            false
        }
    }) {
        unimplemented!("Unexpected character in shell_quote routine: {s:?}")
    }
    if s.contains(' ') {
        format!("'{s}'")
    } else {
        s.to_string()
    }
}

fn shell_quote_command_args<I, S>(args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    args.into_iter()
        .map(|arg| shell_quote(arg.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

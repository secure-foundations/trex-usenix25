use std::io::Write;
use std::path::PathBuf;

use clap::Parser;

use trex::log::*;
use trex::serialize_structural::{Parseable, SerializableStructuralTypes};

use scorer::dsl::{Input, ScoreStats, TestGTPair, RULES};
use scorer::utils::gt_vars_to_test_vars;
use scorer::utils::Var;
use scorer::*;
use trex::structural::StructuralType;

/// A tool to provide statistics about structural types as measured against a ground truth
#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    /// Path to ground truth structural types file, produced using `types2st`
    #[clap(long)]
    ground_truth: PathBuf,
    /// Path to ground truth var map
    #[clap(long)]
    gt_vars: Option<PathBuf>,
    /// Path to structural types file under test, produced by tool being measured
    #[clap(long)]
    test: PathBuf,
    /// Path to test var map
    #[clap(long)]
    test_vars: Option<PathBuf>,
    /// Debug level (repeat for more: 0-warn, 1-info, 2-debug, 3-trace)
    #[clap(short, long, parse(from_occurrences))]
    debug: usize,
    /// Disable terminal logging, even for high severity alerts. Strongly discouraged for normal
    /// use.
    #[clap(long)]
    debug_disable_terminal_logging: bool,
    /// Output to CSV file, if not specified, outputs to stdout.
    #[clap(long)]
    output_csv: Option<PathBuf>,
    /// Output finer-grained results to this CSV file; if not specified, does not output it.
    #[clap(long)]
    output_finer_grained_csv: Option<PathBuf>,
    /// A blunt-instrument configuration flag that (if set to `true`) lets tools that are unable
    /// to produce a type for a variable to be penalized less for it.
    #[clap(long)]
    enable_generous_eval: bool,
}

fn main() {
    let args = Args::parse();
    let _log_guard = slog_scope::set_global_logger(trex::log::FileAndTermDrain::new(
        args.debug,
        args.debug_disable_terminal_logging,
        true,
        None,
    ));
    let (ground_truth, test_data, gt_vars, test_vars) = {
        let gt = utils::read_file_to_string(&args.ground_truth);
        let td = utils::read_file_to_string(&args.test);
        let gtvars = args.gt_vars.as_ref().map(utils::read_file_to_string);
        let tdvars = args.test_vars.as_ref().map(utils::read_file_to_string);
        (gt, td, gtvars, tdvars)
    };

    let ground_truth = SerializableStructuralTypes::<Var>::parse_from(&ground_truth).unwrap();
    let test_data = SerializableStructuralTypes::<Var>::parse_from(&test_data).unwrap();

    let (gt_vars, test_vars) = if let Some(gt_vars) = gt_vars {
        (
            utils::parse_vars(&gt_vars),
            utils::parse_vars(&test_vars.unwrap()),
        )
    } else {
        assert!(test_vars.is_none());
        Default::default()
    };

    let gt_vars_to_test_vars = gt_vars_to_test_vars(&gt_vars, &test_vars);

    let program = args.ground_truth.file_stem().unwrap().to_str().unwrap();
    let mut score_stats = ScoreStats::new(&RULES);

    let mut finer_grained_stats = "Variable,Score,Reason\n".to_owned();

    // We allow mutating `test_data` to support `args.enable_generous_eval`
    let mut test_data = test_data;

    for (var, gti) in ground_truth.var_type_iter() {
        let tdoi = test_data
            .index_of_type_for(gt_vars_to_test_vars.get(var).unwrap_or(var))
            .filter(|&tdi| {
                // Ghidra has no way of marking a variable as "unknown", which means if it knows a
                // variable name, and is unable to give it a type, it will mark the type as
                // `undefined`. For TRex, we have a separate notion of `undefined` and "unknown"
                // (where we output nothing). By ignoring `undefined`s, we thus bring both into
                // sync.
                //
                // If something is marked as undefined, we consider it as the reconstruction tool
                // giving up. Note that this does _not_ mean that `undefined1`/`undefined2`/... is
                // marked as giving up. Instead, only `undefined` is marked as giving up.
                !trex::c_types::is_undefined_padding(test_data.types().get(tdi))
            });

        let tdoi = if args.enable_generous_eval && tdoi.is_none() {
            let typ = StructuralType::default();
            let idx = test_data.types_mut().insert(typ);
            Some(idx)
        } else {
            tdoi
        };

        let res = RULES.compute_one(
            &Input::new(
                TestGTPair {
                    test: &test_data,
                    gt: &&ground_truth,
                },
                TestGTPair {
                    test: tdoi,
                    gt: Some(gti),
                },
            ),
            &mut score_stats,
        );
        if args.output_finer_grained_csv.is_some() {
            finer_grained_stats += &format!(r#""{}",{},"{}""#, var, res.0, res.1.trim());
            finer_grained_stats += "\n";
        }
        trace!(
            "Computed score";
            "trace" => res.1,
            "score" => res.0,
            "variable" => ?var,
        );
    }

    drop(_log_guard); // Ensure that any terminal logging is done before we start printing results

    if let Some(p) = args.output_finer_grained_csv {
        std::fs::File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(p)
            .unwrap()
            .write_all(finer_grained_stats.as_bytes())
            .unwrap();
    }

    if let Some(csv_path) = args.output_csv {
        score_stats.write_to_or_update_csv(&csv_path, program);
    } else {
        println!("{}", score_stats.csv_headings());
        println!("{}", score_stats.to_csv());
    }
}

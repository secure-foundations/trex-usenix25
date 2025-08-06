# Companion Artifact for "TRex: Practical Type Reconstruction for Binary Code"

[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.15611994.svg)](https://doi.org/10.5281/zenodo.15611994)

> [!NOTE]
> This repository contains the tooling to evaluate TRex against prior work, as shown in the evaluation section of [the paper](#publications). For the core TRex tool itself, see the [main repository](https://github.com/secure-foundations/trex).

## Instructions

1. Please install all the [requirements](#requirements).
2. Test that Ghidra is installed correctly, using `just ghidra-test`.
3. Make sure that the [`trex`](https://github.com/secure-foundations/trex) repository has been correctly cloned within the current directory (you can use `just trex` to clone it into the correct path).
4. Run `make` inside the benchmark you want to test (within `benchmarks/`) to populate it with the raw binaries.
5. Run `cargo run --bin runner --release`, and select the benchmark interactively.
  - The runner is an interactive interface we've built for running all the steps of the evaluation conveniently.
  - Choosing the defaults for each prompt should work, but feel free to customize interactively.
    + Caveat: some jobs might expect prior jobs to have already been executed; if they have not, then the job will fail, rather than automatically attempt to run the older job. Such a failure would manifest as a missing file error.
  - The runner automatically caches intermediate results, thus it is safe to stop the runner and re-start it.
    + Note: some jobs may not fully exit cleanly if you simply hit `Ctrl-C`; we have done our best to add safeguards, but if you kill the runner halfway through, it is helpful to run `htop` to confirm that the underlying job has actually been killed.
  - If any job within the runner fails, it is safe to simply re-run the runner.
    + The interactive interface helpfully provides a `jobs-for-benchmark` command after you've made your selections that you can copy-paste to run with the same settings.
    + A failed job _additionally_ provides its own command to run that job individually. While rarely necessary (re-running the whole batch via a `jobs-for-benchmark` command will likely fix things), if there are a large number of failures for a particular job, running an individual command lets you see stdout/stderr for the job, which can help diagnose faults.
  - The runner is aware of CPU and memory usage, and should automatically throttle its parallelism. Nonetheless, providing a limited amount of parallelism might help if you are on a severely constrained machine (at the cost of increased execution time).
6. The results of the runner's jobs will automatically be placed into the relevant `benchmarks/` folder.
  - Summaries show up at the base of the particular benchmark (e.g., `./benchmarks/coreutils/evalfiles/eval-binscores.pdf`).
  - Per-binary results (and intermediary files) show up alongside the binary.

## Requirements

* [Rust](https://www.rust-lang.org/)
* [Just](https://github.com/casey/just)
* [Python](https://www.python.org/)
* [uv](https://docs.astral.sh/uv/)
* [Ghidra](https://github.com/NationalSecurityAgency/ghidra)
  - Must be installed to `/opt/ghidra`.
  - Running `just ghidra-test` will output "Confirmed" if Ghidra is installed successfully.
* [rename](https://packages.ubuntu.com/noble/rename)

See [`./.docker/README.md`](./.docker/README.md) for a `Dockerfile` that
installs the aforementioned requirements.

<details><summary>Known-working versions (click to expand)</summary>

The following versions of the above requirements have been tested. While we
expect code to work on more recent versions, your mileage may vary.

* Rust: 1.86.0
* Just: 1.40.0
* Python: 3.12.2
* uv: 0.5.3
* Ghidra: 10.4
  - **IMPORTANT**: Ghidra will likely require installing a specific version of JDK. Some of the more recent versions of JDK seem to sometimes break Ghidra, thus we recommend using JDK 17. We have tested this version of Ghidra to work successfully with [JDK (17.0.14)](https://www.oracle.com/java/technologies/javase/jdk17-0-13-later-archive-downloads.html). More recent versions of Ghidra may have fixed this issue.
* rename: 2.02

</details>

For the benchmarks (see `benchmarks/`), GNU Coreutils binaries should work out of the box. However, the SPEC CPU 2006 benchmarks require providing a `.tar.xz` at the correct directory. We provide scripts (via Docker) to reproducibly produce binaries from the SPEC CPU source files, but are unable to upload the pre-compiled binaries due to the SPEC CPU license agreement. Please contact [the authors of this paper](#publications) if you need help with running on SPEC CPU 2006.

Note: evaluating ReSym is disabled by default, since it requires access to GPU compute to run within a reasonable amount of time. See more details in [`./tools/evaluating_resym/README.md`](./tools/evaluating_resym/README.md) to enable it. While both are supported, instructions vary based on whether you have a fast GPU locally or remotely.

## License

BSD 3-Clause License. See [LICENSE](./LICENSE).

## Publications

[TRex: Practical Type Reconstruction for Binary Code](TODO-link-to-PDF). Jay Bosamiya, Maverick Woo, and Bryan Parno. In Proceedings of the USENIX Security Symposium, August, 2025.

```bibtex
@inproceedings{trex,
  author    = {Bosamiya, Jay and Woo, Maverick and Parno, Bryan},
  booktitle = {Proceedings of the USENIX Security Symposium},
  month     = {August},
  title     = {{TRex}: Practical Type Reconstruction for Binary Code},
  year      = {2025}
}
```

GHIDRA_HOME := env_var_or_default('GHIDRA_HOME', '/opt/ghidra')
GHIDRA_HEADLESS := 'JAVA_TOOL_OPTIONS="-Dapple.awt.UIElement=true" ' + GHIDRA_HOME + '/support/launch.sh fg jre Ghidra-Headless 8G "-XX:ParallelGCThreads=1 -XX:CICompilerCount=2" ghidra.app.util.headless.AnalyzeHeadless'
GHIDRA_PROJECT_NAME := 'temp-ghidra-project-' + uuid()
GHIDRA_RUN := GHIDRA_HEADLESS + ' /tmp ' + GHIDRA_PROJECT_NAME

# Print available recipes; run by default if no command specified
help:
    @just --list --unsorted

# Confirm basic pre-requisites
confirm-basic-pre-requisites:
    # Checking that `trex` has been cloned (if this fails, you need to run `just trex`)
    @test -d trex/trex
    # Confirming Ghidra setup (if this fails, you need to read the `README.md`)
    @just ghidra-test
    # Basic pre-requisite testing done

# Clone the main TRex code repository into the correct path
trex:
    git clone https://github.com/secure-foundations/trex trex

# Run the main interactive runner
runner:
    cargo run --bin runner --release

######################################################

# Ensure Ghidra exists and works.
ghidra-test:
    # Confirming that Ghidra exists and works
    @{{GHIDRA_HEADLESS}} 2>&1 | grep 'Headless Analyzer Usage: analyzeHeadless' >/dev/null
    # Confirming that it is the version we expect
    @grep "^application.version=10.4$" {{GHIDRA_HOME}}/Ghidra/application.properties >/dev/null
    # Confirmed

# Strip `foo.binar` to produce `foo.ndbg-bin`
strip-binary foo:
    llvm-objcopy --strip-debug {{foo}}.binar {{foo}}.ndbg-bin

# Export PCode from `foo.ndbg-bin`
pcode-export foo:
    {{GHIDRA_RUN}} -import {{foo}}.ndbg-bin -postScript PCodeExporter.java -scriptPath trex/utils/ghidra_headless_scripts/src/ -readOnly 2>&1 {{ if env_var_or_default("DISABLE_GHIDRA_LOG_FILTERING", "f") != "t" { " | grep PCodeExporter " } else { "" } }}
    mv {{file_name(foo)}}.ndbg-bin.pcode-exported {{foo}}.lifted

# Extract variables from `foo.binar`
var-extract foo:
    {{GHIDRA_RUN}} -import {{foo}}.binar -noanalysis -postScript VariableExporter.java -scriptPath trex/utils/ghidra_headless_scripts/src/ -readOnly 2>&1 {{ if env_var_or_default("DISABLE_GHIDRA_LOG_FILTERING", "f") != "t" { "  | grep VariableExporter " } else { "" } }}
    mv {{file_name(foo)}}.binar.var-exported {{foo}}.vars

######################################################

# Export raw Ghidra-decompiled output from `foo.ndbg-bin` and `foo.vars`
decompilation-wvi-export foo:
    {{GHIDRA_RUN}} -import {{foo}}.ndbg-bin -postScript DecompilationDumpWithVariableInputs.java {{foo}}.vars -scriptPath utils/ghidra_headless_scripts/src/ -readOnly 2>&1 {{ if env_var_or_default("DISABLE_GHIDRA_LOG_FILTERING", "f") != "t" { " | grep DecompilationDumpWithVariableInputs " } else { "" } }}
    if grep -F '/**<<EOF>>**/' {{file_name(foo)}}.ndbg-bin.decompilation-wvi-exported >/dev/null; then mv {{file_name(foo)}}.ndbg-bin.decompilation-wvi-exported {{foo}}.decompiled-wvi; else exit 1; fi

# Extract ground-truth type information from `foo.binar`
type-extract foo:
    {{GHIDRA_RUN}} -import {{foo}}.binar -noanalysis -postScript TypesExporter.java -scriptPath utils/ghidra_headless_scripts/src/ -readOnly 2>&1 {{ if env_var_or_default("DISABLE_GHIDRA_LOG_FILTERING", "f") != "t" { " | grep TypesExporter " } else { "" } }}
    mv {{file_name(foo)}}.binar.types-exported {{foo}}.types

# Convert from `foo.types` to `foo.gtst`
gen-struct-types foo:
    cargo run --bin types2st --release --quiet -- {{foo}}.types > {{file_name(foo)}}.gtst
    mv {{file_name(foo)}}.gtst {{foo}}.gtst

# Run trex on `foo.lifted` + relevant `.vars`
run-trex foo extra_args="":
    cargo run --bin trex --release --quiet -- from-ghidra {{foo}}.lifted {{foo}}.vars --output-structural {{foo}}.trex-st --output-c-like {{foo}}.trex-clike {{extra_args}} -ddd --dump-ssa-lifted {{foo}}.trex-ssa --log {{foo}}.trex-log

# Use Ghidra to recover type information from `foo.ndbg-bin` while using relevant `.vars`
run-ghidra-wvi foo: (run-ghidra-wvi-part1 foo) (run-ghidra-wvi-part2 foo)

# Use Ghidra to recover type information from `foo.ndbg-bin` while using relevant `.vars` (part 1: get `types`)
[private]
run-ghidra-wvi-part1 foo:
    {{GHIDRA_RUN}} -import {{foo}}.ndbg-bin -postScript TypesRecovererWithVariableInputs.java {{foo}}.vars -scriptPath utils/ghidra_headless_scripts/src/ -readOnly 2>&1 {{ if env_var_or_default("DISABLE_GHIDRA_LOG_FILTERING", "f") != "t" { " | grep TypesRecovererWithVariableInputs " } else { "" } }}
    mv {{file_name(foo)}}.ndbg-bin.types-recovered-with-var-inputs {{foo}}.ghidra-wvi-types

# Use Ghidra to recover type information from `foo.ndbg-bin` while using relevant `.vars` (part 2: get `st`)
[private]
run-ghidra-wvi-part2 foo:
    cargo run --bin types2st --release --quiet -- {{foo}}.ghidra-wvi-types > {{file_name(foo)}}.ghidra-wvi-st
    mv {{file_name(foo)}}.ghidra-wvi-st {{foo}}.ghidra-wvi-st

# Use ReSym to recover type information from `foo.decompiled-wvi` while using relevant `.vars`
run-resym foo: (run-resym-part1 foo) (run-resym-part2 foo) (run-resym-part3 foo) (run-resym-part4 foo) (run-resym-part5 foo) (run-resym-part6 foo)

# Convert `foo.decompiled-wvi` into a format that ReSym vardecoder will accept (with no mangling)
[private]
run-resym-part1 foo:
    python3 tools/evaluating_resym/convert_to_resym_vardecoder_input_format.py {{foo}}.decompiled-wvi --force-overwrite --output {{foo}}.resym-vardecoder-inp

# Run ReSym vardecoder on `foo.resym-vardecoder-inp`
[private]
run-resym-part2 foo:
    uv run --isolated --with-requirements {{ env_var("RESYM_BASE_DIR") }}/resym/requirements.txt python3 {{ env_var("RESYM_BASE_DIR") }}/resym/training_src/vardecoder_inf.py {{foo}}.resym-vardecoder-inp {{foo}}.resym-vardecoder-out {{ env_var("RESYM_BASE_DIR") }}/vardecoder

# (Remotely) Run ReSym vardecoder on `foo.resym-vardecoder-inp`
[private]
remote-run-resym-part2 foo:
    ./utils/run_command_on_remote.sh -f -i {{foo}}.resym-vardecoder-inp -o {{foo}}.resym-vardecoder-out -n {{ env_var("REMOTE_SERVER") }} -c "uv run --isolated --with-requirements {{ env_var("REMOTE_RESYM_BASE_DIR") }}/resym/requirements.txt python3 {{ env_var("REMOTE_RESYM_BASE_DIR") }}/resym/training_src/vardecoder_inf.py {{foo}}.resym-vardecoder-inp {{foo}}.resym-vardecoder-out {{ env_var("REMOTE_RESYM_BASE_DIR") }}/vardecoder"

# Transform `foo.resym-vardecoder-out` into the input that ReSym needs for fielddecoder.
[private]
run-resym-part3 foo:
    python3 tools/evaluating_resym/convert_to_resym_fielddecoder_input_format.py {{foo}}.resym-vardecoder-out --force-overwrite --output {{foo}}.resym-fielddecoder-inp

# Run ReSym fielddecoder on `foo.resym-fielddecoder-inp`
[private]
run-resym-part4 foo:
    uv run --isolated --with-requirements {{ env_var("RESYM_BASE_DIR") }}/resym/requirements.txt python3 {{ env_var("RESYM_BASE_DIR") }}/resym/training_src/fielddecoder_inf.py {{foo}}.resym-fielddecoder-inp {{foo}}.resym-fielddecoder-out {{ env_var("RESYM_BASE_DIR") }}/fielddecoder

# (Remotely) Run ReSym fielddecoder on `foo.resym-fielddecoder-inp`
[private]
remote-run-resym-part4 foo:
    ./utils/run_command_on_remote.sh -f -i {{foo}}.resym-fielddecoder-inp -o {{foo}}.resym-fielddecoder-out -n {{ env_var("REMOTE_SERVER") }} -c "uv run --isolated --with-requirements {{ env_var("REMOTE_RESYM_BASE_DIR") }}/resym/requirements.txt python3 {{ env_var("REMOTE_RESYM_BASE_DIR") }}/resym/training_src/fielddecoder_inf.py {{foo}}.resym-fielddecoder-inp {{foo}}.resym-fielddecoder-out {{ env_var("REMOTE_RESYM_BASE_DIR") }}/fielddecoder"

# Transform `foo.resym-fielddecoder-out` into C-like types.
[private]
run-resym-part5 foo:
    python3 tools/evaluating_resym/process_resym_output.py {{foo}}.resym-fielddecoder-out {{file_name(foo)}} --force-overwrite --output {{foo}}.resym-types

# Transform `foo.resym-types` into structured types.
[private]
run-resym-part6 foo:
    cargo run --bin types2st --release --quiet -- {{foo}}.resym-types > {{file_name(foo)}}.resym-st
    mv {{file_name(foo)}}.resym-st {{foo}}.resym-st

# Run trivial baseline "reconstructor" on `foo`, only looking at relevant `.vars`
run-baselinetrivial foo:
    cargo run --bin baselinetrivial --release --quiet -- {{foo}}.vars --output {{file_name(foo)}}.baselinetrivial-st
    mv {{file_name(foo)}}.baselinetrivial-st {{foo}}.baselinetrivial-st

# Run scorer on `foo`, for `{ext}-st`, collecting CSV info in `csvdir`
scorer-run ext foo csvdir extra_args="":
    cargo run --bin scorer --release -- --output-csv {{csvdir}}/scorer-{{ext}}.csv --output-finer-grained-csv {{foo}}.scorer-{{ext}} --ground-truth {{foo}}.gtst --test {{foo}}.{{ext}}-st {{extra_args}}

# Run generous scorer on `foo`, for `{ext}-st`, collecting CSV info in `csvdir`
gen-scorer-run ext foo csvdir extra_args="":
    cargo run --bin scorer --release -- --output-csv {{csvdir}}/gen-scorer-{{ext}}.csv --output-finer-grained-csv {{foo}}.gen-scorer-{{ext}} --ground-truth {{foo}}.gtst --test {{foo}}.{{ext}}-st --enable-generous-eval {{extra_args}}

# Compute "standard" true-positive/etc. scores for all `foo.*-st`, storing them into `foo.stdmetrics`
std-metrics foo extra_args="":
    cargo run --bin standardized-scoring --release -- --output-csv {{foo}}.stdmetrics --ground-truth {{foo}}.gtst {{foo}}.*-st {{extra_args}}

# Compute all summarizes for files in `dir` for benchmark named `benchname` and store into `outdir`
summarize-all dir benchname outdir:
    # Summarize standard metrics
    python3 metrics/summarize/summarize_stdmetrics.py {{dir}} {{outdir}}/std-metrics.csv
    # Summarize the scorecounts
    python3 metrics/summarize/summarize_score_counts.py {{dir}} --benchmark-name {{benchname}} --output-figure {{outdir}}/eval-scorecounts.pdf
    # Summarize the generous scorecounts
    ENABLE_GEN=1 python3 metrics/summarize/summarize_score_counts.py {{dir}} --benchmark-name {{benchname}} --output-figure {{outdir}}/geneval-scorecounts.pdf
    # Summarize the binary scores
    python3 metrics/summarize/summarize_binary_scores.py {{dir}} --benchmark-name {{benchname}} --output-figure {{outdir}}/eval-binscores.pdf --output-latex-summary {{outdir}}/summary.tex
    # Summarize the generous binary scores
    python3 metrics/summarize/summarize_binary_scores.py {{dir}} --benchmark-name {{benchname}} --output-figure {{outdir}}/geneval-binscores.pdf

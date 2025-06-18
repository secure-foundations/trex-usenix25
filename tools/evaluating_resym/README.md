# Evaluating ReSym

Evaluating ReSym is disabled by default in the runner, since ReSym requires
access to GPU compute to run within a reasonable amount of time.

The `ENABLE_RESYM=1` environment variable will enable evaluating ReSym. More
setup is necessary, depending on whether you are running ReSym locally on the
same machine as everything else (if you have a fast machine that _also_ has good
GPU support), or if you want to run the GPU-expensive parts of ReSym on a remote
GPU-heavy server.

## Run ReSym purely locally

We recommend this only if your machine already has a powerful GPU or more, with
working CUDA support.

1. Get a [HuggingFace token](https://huggingface.co/docs/hub/security-tokens)
   and set the `HF_TOKEN` environment variable to that.
2. Pick a directory on your machine where you'll place all ReSym-related files.
   The environment variable `RESYM_BASE_DIR` should be set to the _absolute
   path_ of this base directory.
3. Within this base directory:
    - Clone https://github.com/lt-asset/resym/ to get `$RESYM_BASE_DIR/resym`
      (tested with commit `437b4acac21a5a60edecf0c4423f7172f991271f`)
    - Download `vardecoder.zip` from the [ReSym Zenodo
      link](https://zenodo.org/records/15161423), and extract it to get a
      `$RESYM_BASE_DIR/vardecoder` directory.
    - Download `fielddecoder.zip` from the [ReSym Zenodo
      link](https://zenodo.org/records/15161423), and extract it to get
      `$RESYM_BASE_DIR/fielddecoder` directory.
4. Run `cp requirements.txt $RESYM_BASE_DIR/resym/`.
5. Run `cp ./*_inf.py $RESYM_BASE_DIR/resym/training_src/`, overwriting the
   existing scripts there with patched ones.
6. Make sure `ENABLE_RESYM=1` is set.

After following the above steps, the runner will automatically enable the ReSym
jobs too, and the produced summaries include ReSym results as well.

## Run ReSym on remote GPU-heavy server

We've tested this on a server with four A100 GPUs (matching the ReSym paper's
hardware requirements); while this may work on other systems, performance
characteristics may vary.

1. On the remote (server) side:
    1. Follow the local setup steps on the remote server, setting up the
       directory `RESYM_BASE_DIR` _exactly_ as specified above in the
       purely-local setup section.
    2. Make sure that you can SSH to the remote server via simple `ssh
       {SERVERNAME}` (not requiring additional username/password/etc.; you might
       need to set up `.ssh/config` / `.ssh/authorized_keys` for this).
    3. Confirm that `ssh {SERVERNAME} env | grep HF_TOKEN` outputs the
       `HF_TOKEN`. You may need to set `.profile`, `.bashrc`, or similar
       depending on your server setup.
    4. Ensure that `python3` and `uv` are installed on server. The scripts will
       automatically set up a virtual environment with the `requirements.txt` as
       long as these two are installed.
4. On the local (client) side:
    1. Set `REMOTE_SERVER` to the server name (such that `ssh $REMOTE_SERVER`
       works).
    2. Set `REMOTE_RESYM_BASE_DIR` to the server's `RESYM_BASE_DIR`.
    3. Make sure `ENABLE_RESYM=1` is set.
    
After following the above steps, the runner will automatically recognize that
you want to do the GPU-heavy steps remotely, making sure to copy over relevant
files to server, and pulling results back. All other steps are performed
locally.

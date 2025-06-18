# Dockerfile for running the evaluation

The [`Dockerfile`](./Dockerfile) in this directory contains all the
software-requirements necessary to run the evaluation.

For conveniently running this, simply run `make` in this directory, which will
automatically build the Docker image, and drop you into a bash shell inside the
container, connecting in the base repository into `/trex-usenix25`. Exiting out
of the container shell and running `make` again will automatically drop you back
into the same container, so any files or extra files set up inside are persisted
across `make` invocations. To completely close the container (which will erase
such data), use `make kill`. This does not remove the underlying built image
however, to do so, use `make destroy-image`.

Important notes:
- We use [`podman`](https://podman.io/) rather than Docker in the Makefile. This
  means that `root` inside the container directly corresponds to the regular
  user inside, and no permission fixes are necessary if moving files in and out.
- For evaluating against ML-based systems (specifically, for the last subsection
  of the evaluation: Section 5.3 of the paper), due to GPU requirements, we
  recommend setting up ReSym on a remote system with a powerful GPU (as per
  instructions [here](../tools/evaluating_resym/README.md)); specifically, make
  sure that _within the container_ the environment variables `ENABLE_RESYM`,
  `REMOTE_SERVER`, and `REMOTE_RESYM_BASE_DIR` are set as described in the
  instructions, and all the instructions have been correctly followed on the
  powerful-GPU server. If your local system has a powerful GPU, then you can
  simply use the local address as the `REMOTE_SERVER` environment variable too.

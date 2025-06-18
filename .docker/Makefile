# Makefile to simplify using docker/podman. Does a bunch of book-keeping to make
# life easier by correctly starting and stopping containers.
#
# Author: Jay Bosamiya <jayb AT alumni DOT cmu DOT edu>
#
# Version: 0.2.0
#
# Usage:
#
#   Change the CONTAINER_NAME variable if you want a different one
#   from the default.
#
#   [make]               - Magic "do what I mean" command to run.
#   [make kill]          - Kill the current container.
#   [make destroy-image] - Kill the image.

CONTAINER_NAME := trex

# ----------- No need to modify anything below this line ---------
RUNNING_CONTAINER_NAME := .running_container

all: run

run:
	@if [ -f $(RUNNING_CONTAINER_NAME) ]; then \
		echo "[DWIM] Re-running container."; \
		$(MAKE) re-run; \
	else \
		echo "[DWIM] Starting new container."; \
		$(MAKE) first-run; \
	fi

re-run:
	@if [ -f $(RUNNING_CONTAINER_NAME) ]; then \
		podman start -ai $$(cat $(RUNNING_CONTAINER_NAME)); \
	else \
		echo "No container to re-run. Run [make first-run] first."; \
	fi


first-run: build
	@if [ -f $(RUNNING_CONTAINER_NAME) ]; then \
		echo "Already have a running container. If you wanted to re-run, kill previous one using [make kill]."; \
	else \
		echo "temp-`cat /dev/urandom | xxd -p | head -1 | cut -c 1-10`" > $(RUNNING_CONTAINER_NAME); \
		podman run -it -v "$$(pwd)/..:/trex-usenix25" --name $$(cat $(RUNNING_CONTAINER_NAME)) $(CONTAINER_NAME) /bin/bash; \
	fi

kill:
	@if [ -f $(RUNNING_CONTAINER_NAME) ]; then \
		podman rm -f $$(cat $(RUNNING_CONTAINER_NAME)); \
		rm -f $(RUNNING_CONTAINER_NAME); \
	else \
		echo "No running container to kill."; \
	fi

destroy-image: kill
	@if [ -f $(RUNNING_CONTAINER_NAME) ]; then \
		echo "There's a running container. Run [make kill] first."; \
	else \
		echo -n "Destroying image. Are you sure? You have 10 seconds to stop this (Ctrl-C)."; \
		for i in $$(seq 1 10); do sleep 1; echo -n '.'; done; \
		podman rmi $(CONTAINER_NAME); \
	fi

build:
	@if [ -f $(RUNNING_CONTAINER_NAME) ]; then \
		echo "Already have a running container. Run [make kill] first."; \
	else \
		podman build -t $(CONTAINER_NAME) .; \
	fi

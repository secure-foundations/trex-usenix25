RUNNING_CONTAINER_NAME='running_container'
IMAGE_NAME='spec2006-cpu-builder:sp06'

all:
	$(MAKE) confirm-spec
	$(MAKE) copyout
	$(MAKE) kill
	$(MAKE) setup-evalfiles

cpu2006-1_2.tar.xz:
	$(error "[ERROR] No copy of $@ found. Please place a copy of SPEC CPU 2006 in this directory first before continuing.")

confirm-spec: cpu2006-1_2.tar.xz.sha256sum cpu2006-1_2.tar.xz
	@echo "Checking if the checksum for 'cpu2006-1_2.tar.xz' matches the expected"
	sha256sum --check $<

build:
	docker build -t $(IMAGE_NAME) .

run: build
	@if [ -f $(RUNNING_CONTAINER_NAME) ]; then echo 'Run "make kill" first'; exit 1; fi
	@echo "temp-`cat /dev/urandom | xxd -p | head -1 | cut -c 1-10`" > $(RUNNING_CONTAINER_NAME)
	docker run --name $$(cat $(RUNNING_CONTAINER_NAME)) $(IMAGE_NAME) echo Running

copyout: run
	rm -rf bins
	docker cp "$$(cat $(RUNNING_CONTAINER_NAME)):/bins" .

kill:
	@if [ -f $(RUNNING_CONTAINER_NAME) ]; then \
		docker rm -f $$(cat $(RUNNING_CONTAINER_NAME)); \
		rm -f $(RUNNING_CONTAINER_NAME); \
	else \
		echo "No running container to kill."; \
	fi

setup-evalfiles:
	mkdir extractor
	cp bins/*.bin32gcc43 extractor/
	rename 's/\/(.*).bin32gcc43$$/\/\1.binar/' extractor/*
	mkdir -p evalfiles
	mv extractor/*.binar evalfiles/
	rm evalfiles/specrand_base.binar # Remove trivial file that is not part of SPEC even though it is technically produced by it
	rmdir extractor
	$(MAKE) -j$(shell nproc) compressall

compressall: $(patsubst %,%.xz,$(wildcard evalfiles/*.binar))

%.xz: %
	xz -9 $<

.PHONY: all confirm-spec build run

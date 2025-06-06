UID?=$(shell id -u)
GID?=$(shell id -g)

all: confirm-coreutils
	mkdir -p evalfiles
	docker build -t 'builder-coreutils:cu93' .
	docker run --rm -i -v "$$(pwd):/connect" 'builder-coreutils:cu93' bash -c \
		'cp $$(file src/* | grep "ELF" | grep "executable" | awk -F":" "{print \$$1}") /connect/evalfiles && \
		chown $(UID):$(GID) /connect/evalfiles/*'
	rename 's/\/(.*)$$/\/\1.binar/' evalfiles/*
	$(MAKE) -j$(shell nproc) compressall
	docker run --rm -i 'builder-coreutils:cu93' bash -c 'gcc --version'

confirm-coreutils: coreutils-9.3.tar.xz.sha256sum coreutils-9.3.tar.xz
	@echo "Checking if the checksum for '$<' matches the expected"
	sha256sum --check $<

coreutils-9.3.tar.xz:
	wget https://ftp.gnu.org/gnu/coreutils/coreutils-9.3.tar.xz

compressall: $(patsubst %,%.xz,$(wildcard evalfiles/*.binar))

%.xz: %
	xz -9 $<

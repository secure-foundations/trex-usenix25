all:
	mkdir -p evalfiles
	docker build -t builder-basictest .
	docker run --rm -i -v "$$(pwd):/connect" builder-basictest bash -c 'cd /connect && make build-all && chown $(shell id -u):$(shell id -u) evalfiles/*.binar'
	docker run --rm -i -v "$$(pwd):/connect" builder-basictest bash -c 'gcc --version'
	xz -9 evalfiles/*.binar

build-all: $(patsubst src/%.c,evalfiles/%.binar,$(wildcard src/*.c))

evalfiles/%.binar: src/%.c
	gcc -O2 -c -ffunction-sections -fdata-sections -o $@ $< -gdwarf-2

FROM ubuntu:22.04

RUN apt-get update
RUN apt-get install -y build-essential gcc xz-utils tar sed

COPY coreutils-9.3.tar.xz /

RUN tar xvf coreutils-9.3.tar.xz
WORKDIR /coreutils-9.3
RUN FORCE_UNSAFE_CONFIGURE=1 CC=gcc CFLAGS="-gdwarf-2" ./configure
RUN sed -i 's/^MANS = .*$/MANS =/' Makefile
RUN make -j -k

RUN apt-get install -y file

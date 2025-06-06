FROM ubuntu:10.04

ENV DEBIAN_FRONTEND=noninteractive

RUN sed -i 's/archive/old-releases/g' /etc/apt/sources.list
RUN apt-get update

RUN mkdir /spec
COPY cpu2006-1_2.tar.xz /spec
WORKDIR /spec

RUN apt-get install -y gcc-4.3-multilib
RUN apt-get install -y xz-utils

RUN tar xvf ./cpu2006-1_2.tar.xz && rm ./cpu2006-1_2.tar.xz
RUN ./install.sh -f -u linux-suse10-ia32

RUN apt-get install -y patch g++-4.3-multilib gfortran-4.3-multilib
COPY config.patch /spec
RUN cp config/Example-linux64-ia32-gcc43+.cfg config/default.cfg && patch -p0 < config.patch

RUN bash -c "source shrc && runspec --action=build all"
RUN mkdir /bins && find . -name \*bin32gcc43 -exec cp '{}' /bins ';'

CMD bash

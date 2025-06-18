# Base image
FROM ubuntu:24.04

# Just a random value that can be changed to bust the cache. Get a new value using `head -1 /dev/urandom | sha256sum | cut -c1-16`
ENV CACHE_BUSTER=b9b029d5912b0b49

# Make sure that apt-get doesn't prompt for input
ENV DEBIAN_FRONTEND=noninteractive

# Install dependencies
RUN apt-get update
RUN apt-get install -y curl unzip openjdk-17-jdk build-essential git llvm

# Install Rust to a specific toolchain version. More recent versions should
# work, but this one is known to work with the code in this repository.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain 1.86.0 -y

# Install Ghidra
RUN curl -L -o /tmp/ghidra.zip \
    https://github.com/NationalSecurityAgency/ghidra/releases/download/Ghidra_10.4_build/ghidra_10.4_PUBLIC_20230928.zip && \
    unzip /tmp/ghidra.zip -d /opt && \
    ln -s /opt/ghidra_*_PUBLIC/ /opt/ghidra && \
    rm /tmp/ghidra.zip

# Install Just
# https://github.com/casey/just/releases/download/1.40.0/just-1.40.0-x86_64-unknown-linux-musl.tar.gz
RUN curl -L -o /tmp/just.tar.gz \
    https://github.com/casey/just/releases/download/1.40.0/just-1.40.0-x86_64-unknown-linux-musl.tar.gz && \
    tar -xzf /tmp/just.tar.gz -C /usr/local/bin && \
    rm /tmp/just.tar.gz

# Install uv
RUN curl -LsSf https://astral.sh/uv/install.sh | sh

# Add the Rust toolchain and uv to the PATH
ENV PATH="/root/.cargo/bin:/root/.local/bin:${PATH}"

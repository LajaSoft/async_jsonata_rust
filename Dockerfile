FROM node:20-bullseye

ENV RUSTUP_HOME=/opt/rust \
    CARGO_HOME=/opt/rust \
    PATH=/opt/rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libssl-dev \
        python3 \
        ca-certificates \
        curl && \
    rm -rf /var/lib/apt/lists/*

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path && \
    corepack enable pnpm

WORKDIR /workspace

CMD ["/bin/bash"]

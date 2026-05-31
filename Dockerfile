# Minimal Rust dev image for the async_jsonata_rust crate.
# (The old Node.js/pnpm toolchain was removed together with the JS sources.)
FROM debian:bookworm-slim

ENV RUSTUP_HOME=/opt/rust \
    CARGO_HOME=/opt/rust \
    PATH=/opt/rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl && \
    rm -rf /var/lib/apt/lists/*

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path

WORKDIR /workspace

CMD ["/bin/bash"]

# A clean image that builds and runs the orders_report example using the
# async_jsonata_rust crate pulled from crates.io (NOT from this repo).
FROM rust:1-slim AS build
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
# Fetches async_jsonata_rust from crates.io and builds the binary.
RUN cargo build --release

# Minimal runtime image with just the compiled binary.
FROM debian:bookworm-slim
COPY --from=build /app/target/release/examples-app /usr/local/bin/examples-app
CMD ["examples-app"]

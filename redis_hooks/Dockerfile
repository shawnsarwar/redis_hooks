ARG  REDIS_VERSION=${REDIS_VERSION}

FROM lukemathwalker/cargo-chef:latest-rust-1-bullseye AS chef
RUN apt-get update
RUN apt-get install -y gcc make clang
WORKDIR /build
COPY ./src ./src
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
RUN cargo chef prepare --recipe-path recipe.json

FROM chef as build
COPY --from=chef /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY ./src ./src
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
RUN cargo build --release

FROM scratch as bin-release
COPY --from=build /build/target/release/libredis_hooks.so /libredis_hooks.so

FROM redis:$REDIS_VERSION as redis
# COPY --from=build /build/target/release/libredis_hooks.so /plugins/release/libredis_hooks.so
RUN ls -la
COPY --from=bin-release /libredis_hooks.so /plugins/release/libredis_hooks.so


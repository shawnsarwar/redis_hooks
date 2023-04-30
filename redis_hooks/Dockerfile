ARG  REDIS_VERSION=${REDIS_VERSION}

FROM rust:1.69-slim-bullseye as rust
WORKDIR /build
RUN cargo install cargo-chef
RUN apt-get update
RUN apt-get install -y gcc make clang

FROM rust as planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust as deps
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .

FROM deps as build
COPY --from=deps /build /build
RUN cargo build --release

FROM redis:$REDIS_VERSION as redis
COPY --from=build /build/target/release/libredis_hooks.so /plugins/release/libredis_hooks.so

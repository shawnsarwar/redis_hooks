FROM rust:1.69-slim-bullseye as rust
WORKDIR /build
RUN cargo install cargo-chef

FROM rust as planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust as builder
RUN apt-get update
RUN apt-get install -y gcc make clang
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
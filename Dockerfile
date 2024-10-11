FROM rustlang/rust:nightly as builder
ARG TARGET_CPU="native"
ARG TARGET_FEATURES=""
ARG CARGO_ARGS=""
WORKDIR /usr/src/iqdb-rs
COPY lib/ lib/
COPY server/ server/
COPY Cargo.toml rust-toolchain.toml ./
RUN RUSTFLAGS="-C target-feature=+crt-static,${TARGET_FEATURES} -C target-cpu=${TARGET_CPU}" cargo build --profile release-lto --target x86_64-unknown-linux-gnu ${CARGO_ARGS}

FROM scratch
WORKDIR /iqdb
COPY --from=builder /usr/src/iqdb-rs/target/x86_64-unknown-linux-gnu/release-lto/iqdb-server /iqdb-server

EXPOSE 5588
ENTRYPOINT ["/iqdb-server"]
CMD ["--database", "iqdb.sqlite"]

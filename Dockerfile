FROM rustlang/rust:nightly as builder
WORKDIR /usr/src/iqdb-rs
COPY lib/ lib/
COPY server/ server/
COPY Cargo.toml rust-toolchain.toml ./
RUN RUSTFLAGS="-C target-feature=+crt-static" cargo build --release --target x86_64-unknown-linux-gnu

FROM alpine:3.19.1
WORKDIR /iqdb
COPY --from=builder /usr/src/iqdb-rs/target/x86_64-unknown-linux-gnu/release/iqdb-server /usr/local/bin/iqdb-server

EXPOSE 5588
ENTRYPOINT ["iqdb-server"]
CMD ["--database", "iqdb.db"]

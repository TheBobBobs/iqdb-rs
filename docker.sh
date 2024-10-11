#!/bin/sh

sudo docker build --tag bobbobs/iqdb-rs:latest \
  --build-arg TARGET_CPU="x86-64-v3" \
  --build-arg TARGET_FEATURES="" \
  --build-arg CARGO_ARGS="--no-default-features" .
sudo docker build --tag bobbobs/iqdb-rs:latest-mt \
  --build-arg TARGET_CPU="x86-64-v3" \
  --build-arg TARGET_FEATURES="" \
  --build-arg CARGO_ARGS="--features multi-thread" .

sudo docker build --tag bobbobs/iqdb-rs:x86_64-v4 \
  --build-arg TARGET_CPU="x86-64-v4" \
  --build-arg TARGET_FEATURES="" \
  --build-arg CARGO_ARGS="--no-default-features" .
sudo docker build --tag bobbobs/iqdb-rs:x86_64-v4-mt \
  --build-arg TARGET_CPU="x86-64-v4" \
  --build-arg TARGET_FEATURES="" \
  --build-arg CARGO_ARGS="--features multi-thread" .

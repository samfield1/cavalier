FROM rust:1.87-slim-bookworm AS builder

ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup target add wasm32-unknown-unknown
RUN cargo install wasm-bindgen-cli
RUN cargo install --locked --root /usr/bin/ trunk

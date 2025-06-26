FROM rust:alpine3.22 AS builder

WORKDIR /app

RUN apk add --no-cache \
    llvm-dev \
    clang18-static \
    musl-dev \
    pkgconfig \
    perl \
    make \
    clang-dev \
    libstdc++ \
    musl

COPY worker .

RUN --mount=type=cache,target=/app/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --target x86_64-unknown-linux-musl --release && \
    mv target/x86_64-unknown-linux-musl/release/nur_worker /app/nur_worker

FROM alpine:3.22

RUN apk add --no-cache \
    clang-dev

WORKDIR /

COPY --from=builder /app/nur_worker /nur_worker

CMD ["/nur_worker"]

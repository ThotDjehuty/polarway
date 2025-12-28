# syntax=docker/dockerfile:1.4
# Fast optimized build for Polaroid gRPC server

FROM rustlang/rust:nightly-slim AS builder

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_BUILD_JOBS=8 \
    CARGO_INCREMENTAL=0 \
    RUSTFLAGS="-C codegen-units=16 -C opt-level=3"

# Install build dependencies
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config libssl-dev protobuf-compiler libprotobuf-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy all source code (simpler, avoid partial copy issues)
COPY . .

# Build in release mode with optimizations
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cd polaroid-grpc && \
    cargo build --release --target-dir /build/target && \
    cp /build/target/release/polaroid-grpc /polaroid-grpc

# Runtime stage - minimal image
FROM debian:bookworm-slim

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    libssl3 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /polaroid-grpc /app/polaroid-grpc

# Default bind to all interfaces
ENV POLAROID_BIND_ADDRESS=0.0.0.0:50052

EXPOSE 50052

# Simple health check using nc
RUN apt-get update && apt-get install -y --no-install-recommends netcat-openbsd && rm -rf /var/lib/apt/lists/*

CMD ["/app/polaroid-grpc"]

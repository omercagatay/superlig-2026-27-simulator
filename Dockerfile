# Stage 1: Build Rust backend
FROM rust:slim-bookworm AS backend-builder
WORKDIR /app
RUN apt-get update && apt-get install -y build-essential && rm -rf /var/lib/apt/lists/*

# Cache dependencies: build a dummy binary first so cargo compiles all crates
# into a Docker layer that only invalidates when Cargo.toml/Cargo.lock change.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -rf src target/release/deps/superlig_sim* target/release/superlig-sim* \
           target/release/.fingerprint/superlig_sim* target/release/incremental/superlig_sim*

# Real build: only our crate recompiles; dependencies come from the cached layer.
COPY data/ ./data/
COPY src/ ./src/
RUN cargo build --release

# Stage 2: Build React frontend
FROM node:20-slim AS frontend-builder
WORKDIR /app
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# Stage 3: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend-builder /app/target/release/superlig-sim .
COPY --from=frontend-builder /app/dist ./frontend/dist
EXPOSE 3000
CMD ["./superlig-sim"]

# --- build stage ---
FROM rust:1.91-slim AS builder

WORKDIR /app

# Install system dependencies required for openssl-sys and C++ linked crates (ort, tokenizers)
RUN apt-get update \
    && apt-get install -y pkg-config libssl-dev g++ \
    && rm -rf /var/lib/apt/lists/*

# Stub both binaries so Cargo can resolve and cache all dependencies
# before we copy real source. This avoids re-downloading crates on
# every source change.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src/bin \
    && echo "fn main() {}" > src/main.rs \
    && echo "fn main() {}" > src/bin/seed.rs \
    && cargo build --release \
    && rm -rf src

# Copy real source and data, then do the real build
COPY src ./src
RUN touch src/main.rs src/bin/seed.rs && cargo build --release

# The `ort` crate with download-binaries links against a shared libonnxruntime.
# Collect it into a predictable path so the runtime stage can copy it cleanly.
RUN mkdir /app/ort_libs \
    && find /app/target/release/build -name "libonnxruntime.so*" \
    -exec cp -P {} /app/ort_libs/ \;

# --- runtime stage ---
FROM debian:trixie-slim

# Install runtime dependencies: 
# ca-certificates for HTTPS, wget for downloading the ONNX model,
# and libssl3 for the OpenSSL bindings compiled in the backend/seeder.
RUN apt-get update \
    && apt-get install -y ca-certificates wget libssl3 \
    && rm -rf /var/lib/apt/lists/*

# ORT shared library — must be on the dynamic linker path before the
# seed or backend binaries are executed.
COPY --from=builder /app/ort_libs/ /usr/local/lib/
RUN ldconfig

COPY --from=builder /app/target/release/backend /usr/local/bin/backend
COPY --from=builder /app/target/release/seed    /usr/local/bin/seed

# Download all-MiniLM-L6-v2 ONNX model and tokenizer from HuggingFace.
# These are fetched at image-build time so the seeder has no outbound
# dependency at container start.
RUN mkdir -p /app/models \
    && wget -q -O /app/models/model.onnx \
    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx" \
    && wget -q -O /app/models/tokenizer.json \
    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json"

WORKDIR /app

EXPOSE 3001
CMD ["backend"]
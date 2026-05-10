# AEGIS Docker Image - Phase 2

FROM rust:1.75 as builder

WORKDIR /build

# Copy workspace
COPY . .

# Build in release mode
RUN cargo build --release -p aegis-runtime

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/release/aegis-runtime /app/

# Create directories
RUN mkdir -p /models /var/lib/aegis

# Expose ports
EXPOSE 50051 9001 9090

# Health check
HEALTHCHECK --interval=10s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:9001/health || exit 1

# Run AEGIS node
ENTRYPOINT ["/app/aegis-runtime"]

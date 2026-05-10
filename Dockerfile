# Multi-stage Dockerfile for AEGIS Scheduler

# Build stage
FROM rust:1.75.0 as builder

WORKDIR /build

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    cmake \
    git \
    && rm -rf /var/lib/apt/lists/*

# Copy source code
COPY . .

# Build application
ARG PROFILE=release
RUN cargo build --${PROFILE} \
    && mv target/${PROFILE}/aegis-scheduler /usr/local/bin/

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 -s /sbin/nologin aegis

# Create data directories with correct permissions
RUN mkdir -p /var/lib/aegis/{wal,snapshots} && \
    chown -R aegis:aegis /var/lib/aegis && \
    chmod 700 /var/lib/aegis

# Copy binary from builder
COPY --from=builder /usr/local/bin/aegis-scheduler /usr/local/bin/

# Set working directory
WORKDIR /var/lib/aegis

# Switch to non-root user
USER aegis

# Health check
HEALTHCHECK --interval=10s --timeout=5s --retries=3 --start-period=30s \
    CMD curl -f http://localhost:8000/health || exit 1

# Expose ports
EXPOSE 6000 8000

# Run application
ENTRYPOINT ["/usr/local/bin/aegis-scheduler"]
CMD []

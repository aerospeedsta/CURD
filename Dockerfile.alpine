# --- Efficient Runtime-Only Image for CURD ---
# This Dockerfile expects binaries to be pre-built (e.g., via cargo-zigbuild)
# and passed in during context creation or copied from a specific path.

FROM alpine:latest
ARG TARGETARCH
RUN apk add --no-cache git ca-certificates

COPY curd-*-static ./
RUN if [ "$TARGETARCH" = "amd64" ]; then mv curd-x86_64-static /usr/local/bin/curd; \
    elif [ "$TARGETARCH" = "arm64" ]; then mv curd-aarch64-static /usr/local/bin/curd; fi
RUN chmod +x /usr/local/bin/curd

# Default workspace
WORKDIR /workspace

# MCP Default: Stdin/Stdout
ENTRYPOINT ["/usr/local/bin/curd"]
CMD ["mcp", "."]

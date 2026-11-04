FROM golang:1.25-alpine AS builder

# Build the official RELEASE.2025-10-15T17-29-55Z source; public MinIO images
# are no longer available. Pin the commit rather than following a moving tag.
RUN CGO_ENABLED=0 GOBIN=/out go install -trimpath -ldflags="-s -w" \
    github.com/minio/minio@9e49d5e7a648f00e26f2246f4dc28e6b07f8c84a

FROM alpine:3.22
RUN apk add --no-cache ca-certificates
COPY --from=builder /out/minio /usr/local/bin/minio
EXPOSE 9000
ENTRYPOINT ["/usr/local/bin/minio"]

# docker/egress-proxy.Dockerfile - versioned Tinyproxy runtime
# The proxy is built once with the release and is not installed from the live
# Alpine package index during service startup.
FROM alpine:3.20@sha256:d9e853e87e55526f6b2917df91a2115c36dd7c696a35be12163d44e6e2a4b6bc

# Keep the proxy package explicit so a rebuild cannot silently move to a new
# runtime or configuration default.
RUN apk add --no-cache tinyproxy=1.11.2-r0

COPY docker/egress-proxy.conf /etc/tinyproxy/tinyproxy.conf

USER tinyproxy

EXPOSE 8888

ENTRYPOINT ["tinyproxy", "-d", "-c", "/etc/tinyproxy/tinyproxy.conf"]

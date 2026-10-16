# One image, every host (spikes 021-023, 031): the `apr` CPU RELEASE binary for aarch64 Linux
# (sha256-verified by `just llm-endpoint-image` before it ever reaches this build context), the
# Lambda Web Adapter (a Lambda extension that proxies invocations to the local HTTP server; inert on
# Fargate/ECS), and the AWS CLI to pull the GGUF from S3 at start.
#
# No weights and no secret enter the image: the key hash, the S3 URI of the model and the model's
# sha256 are FUNCTION ENVIRONMENT VARIABLES, set by the deploy, never by this file. Weights pulled at
# run time land at MODEL_LOCAL below (a destination path, not weights).
#
# Build context is deploy/llm-endpoint/ and `bin/apr` is the extracted, gitignored release binary.
# The adapter digest literal below must equal LWA_IMAGE_DIGEST in pin.env (a Dockerfile cannot read
# the env file); `just llm-endpoint-image` refuses (lwa-digest) when the two copies differ.
FROM public.ecr.aws/awsguru/aws-lambda-adapter:0.9.1@sha256:46d6625e68cbbdd2efab4a20245977664513f13ffef47915b000d431adcea0b4 AS lwa

FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal

ARG APR_TAG
ARG APR_TARBALL_SHA256
LABEL org.aprender.apr_tag=$APR_TAG \
      org.aprender.apr_tarball_sha256=$APR_TARBALL_SHA256 \
      org.aprender.phase=09

RUN microdnf install -y awscli-2 procps-ng && microdnf clean all
COPY --from=lwa /lambda-adapter /opt/extensions/lambda-adapter
COPY bin/apr /usr/local/bin/apr
COPY entrypoint.sh /entrypoint.sh
RUN chmod 0755 /usr/local/bin/apr /entrypoint.sh && /usr/local/bin/apr --version

# Lambda Web Adapter: forward every invocation to :8080 once /health answers. Readiness is any
# status 100-499 for LWA 0.9.1, so /health answering 401 (the key is required) still counts as ready.
# ASYNC_INIT lets the model download + load outrun Lambda's 10 s init phase (readiness continues in
# the first invoke).
ENV AWS_LWA_PORT=8080 \
    AWS_LWA_READINESS_CHECK_PATH=/health \
    AWS_LWA_READINESS_CHECK_PROTOCOL=http \
    AWS_LWA_ASYNC_INIT=true \
    AWS_LWA_INVOKE_MODE=buffered \
    PORT=8080 \
    MODEL_LOCAL=/tmp/model.gguf \
    RAYON_NUM_THREADS=6 \
    HOME=/tmp
EXPOSE 8080
ENTRYPOINT ["/entrypoint.sh"]

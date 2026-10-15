# One image, every host (CONVENTIONS, spikes 021-023): the v0.70.3 `apr` CPU release binary for
# aarch64 Linux (sha256-verified from the GitHub release), the Lambda Web Adapter (a Lambda
# extension that proxies invocations to the local HTTP server; inert on Fargate/ECS), and the
# AWS CLI to pull the GGUF from S3 at start. Weights never go in the image (021: 7-18x worse).
FROM public.ecr.aws/awsguru/aws-lambda-adapter:0.9.1 AS lwa

FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal
RUN microdnf install -y awscli-2 procps-ng && microdnf clean all
COPY --from=lwa /lambda-adapter /opt/extensions/lambda-adapter
COPY apr /usr/local/bin/apr
COPY entrypoint.sh /entrypoint.sh
RUN chmod 0755 /usr/local/bin/apr /entrypoint.sh && /usr/local/bin/apr --version

# Lambda Web Adapter: forward every invocation to :8080 once /health answers 200. ASYNC_INIT lets the
# model download + load outrun Lambda's 10 s init phase (readiness continues in the first invoke).
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

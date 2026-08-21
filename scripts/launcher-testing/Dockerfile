FROM ubuntu

RUN apt-get update -y && apt-get install -y curl wget vim rustup

# Install openclaw
RUN export OPENCLAW_NO_ONBOARD=1 && \
    export OPENCLAW_NO_PROMPT=1 && \
    curl -fsSL https://openclaw.ai/install.sh | bash

# Install hermes
RUN curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash -s -- --non-interactive --skip-browser --skip-setup

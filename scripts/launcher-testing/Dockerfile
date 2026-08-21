FROM ubuntu

RUN apt-get update -y && apt-get install -y curl wget vim rustup

# Install openclaw
RUN export OPENCLAW_NO_ONBOARD=1 && \
    export OPENCLAW_NO_PROMPT=1 && \
    curl -fsSL https://openclaw.ai/install.sh | bash

# Install hermes
RUN curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash -s -- --non-interactive --skip-browser --skip-setup

# Install goose
RUN curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | CONFIGURE=false GOOSE_BIN_DIR=/usr/local/bin bash

# Install claude
RUN curl -fsSL https://claude.ai/install.sh | bash && \
    chmod -R ugo+rx /root && \
    mv /root/.local/bin/claude /usr/local/bin/claude && \
    chmod ugo+rx /usr/local/bin/claude
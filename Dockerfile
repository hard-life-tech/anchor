# ---------- build stage ----------
FROM rust:1-slim-bookworm AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache dependencies before copying sources.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release \
    && rm -rf src

COPY . .
RUN touch src/main.rs \
    && rm -f target/release/anchor target/release/deps/anchor-* \
    && cargo build --release

# ---------- runtime stage ----------
FROM debian:bookworm-slim

ARG AGENT_UID=1000
ARG AGENT_GID=1000

RUN apt-get update && apt-get install -y --no-install-recommends \
      git tmux openssh-client ca-certificates curl gnupg \
    && rm -rf /var/lib/apt/lists/*

# Node.js — required by OpenCode's installer.
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# OpenCode (global npm) — lands on default PATH for all users.
RUN npm install -g opencode-ai

# Cursor CLI installer writes to $HOME/.local. The runtime volume mounts over
# /home/agent, so keep the binary tree under /usr/local (outside the volume).
RUN curl -fsSL https://cursor.com/install | bash \
    && mkdir -p /usr/local/share \
    && cp -a /root/.local/share/cursor-agent /usr/local/share/cursor-agent \
    && LATEST="$(ls -1 /usr/local/share/cursor-agent/versions | sort | tail -1)" \
    && ln -sfn "/usr/local/share/cursor-agent/versions/${LATEST}/cursor-agent" /usr/local/bin/agent \
    && ln -sfn /usr/local/bin/agent /usr/local/bin/cursor-agent \
    && agent --version \
    && cursor-agent --version

RUN groupadd -g ${AGENT_GID} agent \
    && useradd -m -u ${AGENT_UID} -g ${AGENT_GID} -s /bin/bash agent

ENV HOME=/home/agent
# /usr/local/bin already has agent + cursor-agent; keep ~/.local/bin for
# operator-installed tools that persist on the agent-home volume.
ENV PATH="/usr/local/bin:/home/agent/.local/bin:/home/agent/.cursor/bin:${PATH}"

COPY --from=builder /app/target/release/anchor /usr/local/bin/anchor

USER agent
WORKDIR /home/agent
VOLUME ["/home/agent"]
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/anchor"]

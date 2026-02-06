# OpenClaw Agent Container
# Build with: podman build -t openclaw-agent -f Containerfile .
# Run with:   podman run -d --name openclaw-agent openclaw-agent

FROM ubuntu:24.04

# Set non-interactive frontend for apt
ENV DEBIAN_FRONTEND=noninteractive

# Install dependencies for Homebrew and OpenClaw
RUN apt-get update && apt-get install -y \
    build-essential \
    procps \
    curl \
    file \
    git \
    bash \
    ca-certificates \
    sudo \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user for Homebrew (it doesn't like running as root)
RUN useradd -m -s /bin/bash brewuser && \
    echo "brewuser ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Switch to brewuser for Homebrew installation
USER brewuser
WORKDIR /home/brewuser

# Install Homebrew non-interactively
RUN NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Add Homebrew to PATH
ENV PATH="/home/linuxbrew/.linuxbrew/bin:/home/linuxbrew/.linuxbrew/sbin:$PATH"

# Verify Homebrew installation
RUN brew --version

# Download OpenClaw installer (will be run interactively later)
RUN curl -fsSL https://openclaw.ai/install.sh -o /tmp/install-openclaw.sh && chmod +x /tmp/install-openclaw.sh

# Set working directory
WORKDIR /app

# Keep container running for exec commands
CMD ["bash", "-c", "while true; do sleep 3600; done"]

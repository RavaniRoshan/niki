FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    git curl build-essential pkg-config libssl-dev python3 \
    nodejs npm \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Clone NIKI from GitHub
RUN git clone https://github.com/RavaniRoshan/niki.git /niki

WORKDIR /niki

# Build release binary
RUN cargo build --release

# Configure git identity for the test project commit
RUN git config --global user.email "test@niki.dev" && \
    git config --global user.name "Niki Test"

# Create a sample test project inside the container
RUN mkdir -p /test-project && cd /test-project && \
    git init && \
    echo '# Test App\n\nconsole.log("hello");' > index.js && \
    echo 'node_modules/\n.niki/' > .gitignore && \
    git add . && git commit -m "initial"

# Copy mock LLM servers
COPY tests/integration/mock_llm.py /usr/local/bin/mock_llm.py
COPY tests/integration/minimal_mock.py /usr/local/bin/minimal_mock.py

# Create niki config pointing at mock LLM
RUN mkdir -p /test-project/.niki
COPY tests/integration/niki.test.toml /test-project/niki.toml

WORKDIR /test-project
CMD ["/bin/bash"]

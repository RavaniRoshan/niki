BUILD_DIR := bin
BINARY := nikicode
# Personal install: ~/.local/bin by default, override with PREFIX=.
PREFIX ?= $(HOME)/.local/bin

all: build

build:
	go build -o $(BUILD_DIR)/$(BINARY) ./cmd/nikicode

# One-command local install for this machine: the binary plus the
# `nc` alias and the `niki` compat symlink. Creates nothing else.
install: build
	mkdir -p $(PREFIX)
	cp -f $(BUILD_DIR)/$(BINARY) $(PREFIX)/$(BINARY)
	ln -sf $(BINARY) $(PREFIX)/nc
	ln -sf $(BINARY) $(PREFIX)/niki
	$(PREFIX)/$(BINARY) --version
	@echo "installed $(BINARY), nc, niki -> $(PREFIX)"

test:
	go test ./...

vet:
	go vet ./...

lint: vet

run:
	go run ./cmd/nikicode

clean:
	rm -rf $(BUILD_DIR)

.PHONY: all build install test vet lint run clean

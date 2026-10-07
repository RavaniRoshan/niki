BUILD_DIR := bin

all: build

build:
	go build -o $(BUILD_DIR)/niki ./cmd/niki

test:
	go test ./...

vet:
	go vet ./...

lint: vet

run:
	go run ./cmd/niki

clean:
	rm -rf $(BUILD_DIR)

.PHONY: all build test vet lint run clean

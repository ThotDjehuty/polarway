.DEFAULT_GOAL := help

PYTHONPATH=
SHELL=bash
ifeq ($(VENV),)
VENV := .venv
endif

RUNTIME_CARGO_TOML=py-polars/runtime/polars-runtime-32/Cargo.toml

# Detect if running from rust-arblab context
ARBLAB_DIR := $(shell if [ -f "../rust-arblab/docker-compose.yml" ]; then echo "../rust-arblab"; fi)

ifeq ($(OS),Windows_NT)
	VENV_BIN=$(VENV)/Scripts
else
	VENV_BIN=$(VENV)/bin
endif

# Detect CPU architecture.
ifeq ($(OS),Windows_NT)
    ifeq ($(PROCESSOR_ARCHITECTURE),AMD64)
		ARCH := amd64
	else ifeq ($(PROCESSOR_ARCHITECTURE),x86)
		ARCH := x86
	else ifeq ($(PROCESSOR_ARCHITECTURE),ARM64)
		ARCH := arm64
	else
		_DUMMY := $(warning Unknown architecture '$(PROCESSOR_ARCHITECTURE)', defaulting to 'unknown')
		ARCH := unknown
    endif
else
    UNAME_M := $(shell uname -m)
    ifeq ($(UNAME_M),x86_64)
		ARCH := amd64
    else ifeq ($(UNAME_M),x64)
		ARCH := amd64
    else ifeq ($(UNAME_M),arm64)
		ARCH := arm64
    else ifeq ($(UNAME_M),aarch64)
		ARCH := arm64
	else ifneq ($(filter %86,$(UNAME_M)),)
		ARCH := x86
	else
		_DUMMY := $(warning Unknown architecture '$(UNAME_M)', defaulting to 'unknown')
		ARCH := unknown
    endif
endif

# Ensure boolean arguments are normalized to 1/0 to prevent surprises.
ifdef LTS_CPU
	ifeq ($(LTS_CPU),0)
	else ifeq ($(LTS_CPU),1)
	else
$(error LTS_CPU must be 0 or 1 (or undefined, default to 0))
	endif
endif

# Define RUSTFLAGS and CFLAGS appropriate for the architecture.
# Keep synchronized with .github/workflows/release-python.yml.
ifeq ($(ARCH),amd64)
	ifeq ($(LTS_CPU),1)
		FEAT_RUSTFLAGS=-C target-feature=+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cmpxchg16b
		FEAT_CFLAGS=-msse3 -mssse3 -msse4.1 -msse4.2 -mpopcnt -mcx16
	else
		FEAT_RUSTFLAGS=-C target-feature=+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cmpxchg16b,+avx,+avx2,+fma,+bmi1,+bmi2,+lzcnt,+pclmulqdq,+movbe -Z tune-cpu=skylake
		FEAT_CFLAGS=-msse3 -mssse3 -msse4.1 -msse4.2 -mpopcnt -mcx16 -mavx -mavx2 -mfma -mbmi -mbmi2 -mlzcnt -mpclmul -mmovbe -mtune=skylake
	endif
endif

override RUSTFLAGS+=$(FEAT_RUSTFLAGS)
override CFLAGS+=$(FEAT_CFLAGS)
export RUSTFLAGS
export CFLAGS

# Define command to filter pip warnings when running maturin
FILTER_PIP_WARNINGS=| grep -v "don't match your environment"; test $${PIPESTATUS[0]} -eq 0

.venv:  ## Set up Python virtual environment and install requirements
	$(MAKE) requirements

# Note: Installed separately as pyiceberg does not have wheels for 3.13, causing
# --no-build to fail.
.PHONY: requirements
requirements:  ## Install/refresh Python project requirements
	@unset CONDA_PREFIX \
	&& python3 -m venv $(VENV) --clear \
	&& $(VENV_BIN)/python -m pip install --upgrade uv \
	&& $(VENV_BIN)/uv pip install --upgrade --compile-bytecode --no-build \
	   -r py-polars/requirements-dev.txt \
	   -r py-polars/requirements-lint.txt \
	   -r py-polars/docs/requirements-docs.txt \
	   -r docs/source/requirements.txt \
	&& $(VENV_BIN)/uv pip install --upgrade --compile-bytecode "pyiceberg>=0.7.1" pyiceberg-core \
	&& $(VENV_BIN)/uv pip install --no-deps -e py-polars \
	&& $(VENV_BIN)/uv pip uninstall polars-runtime-compat polars-runtime-64  ## Uninstall runtimes which might take precedence over polars-runtime-32

.PHONY: requirements-all
requirements-all:  ## Install/refresh all Python requirements (including those needed for CI tests)
	$(MAKE) requirements
	$(VENV_BIN)/uv pip install --upgrade --compile-bytecode -r py-polars/requirements-ci.txt

.PHONY: build
build: .venv  ## Compile and install Python Polars for development
	@unset CONDA_PREFIX \
	&& $(VENV_BIN)/maturin develop -m $(RUNTIME_CARGO_TOML) $(ARGS) --uv \
	$(FILTER_PIP_WARNINGS)

.PHONY: build-mindebug
build-mindebug: .venv  ## Same as build, but don't include full debug information
	@unset CONDA_PREFIX \
	&& $(VENV_BIN)/maturin develop -m $(RUNTIME_CARGO_TOML) --profile mindebug-dev $(ARGS) --uv \
	$(FILTER_PIP_WARNINGS)

.PHONY: build-release
build-release: .venv  ## Compile and install Python Polars binary with optimizations, with minimal debug symbols
	@unset CONDA_PREFIX \
	&& $(VENV_BIN)/maturin develop -m $(RUNTIME_CARGO_TOML) --release $(ARGS) --uv \
	$(FILTER_PIP_WARNINGS)

.PHONY: build-nodebug-release
build-nodebug-release: .venv  ## Same as build-release, but without any debug symbols at all (a bit faster to build)
	@unset CONDA_PREFIX \
	&& $(VENV_BIN)/maturin develop -m $(RUNTIME_CARGO_TOML) --profile nodebug-release $(ARGS) --uv \
	$(FILTER_PIP_WARNINGS)

.PHONY: build-debug-release
build-debug-release: .venv  ## Same as build-release, but with full debug symbols turned on (a bit slower to build)
	@unset CONDA_PREFIX \
	&& $(VENV_BIN)/maturin develop -m $(RUNTIME_CARGO_TOML) --profile debug-release $(ARGS) --uv \
	$(FILTER_PIP_WARNINGS)

.PHONY: build-dist-release
build-dist-release: .venv  ## Compile and install Python Polars binary with super slow extra optimization turned on, for distribution
	@unset CONDA_PREFIX \
	&& $(VENV_BIN)/maturin develop -m $(RUNTIME_CARGO_TOML) --profile dist-release $(ARGS) --uv \
	$(FILTER_PIP_WARNINGS)

.PHONY: check
check:  ## Run cargo check with all features
	cargo check --workspace --all-targets --all-features

.PHONY: clippy
clippy:  ## Run clippy with all features
	cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -D clippy::dbg_macro

.PHONY: clippy-default
clippy-default:  ## Run clippy with default features
	cargo clippy --all-targets --locked -- -D warnings -D clippy::dbg_macro

.PHONY: fmt
fmt:  ## Run autoformatting and linting
	$(VENV_BIN)/ruff check
	$(VENV_BIN)/ruff format
	cargo fmt --all
	dprint fmt
	$(VENV_BIN)/typos

.PHONY: fix
fix:
	cargo clippy --workspace --all-targets --all-features --fix
	@# Good chance the fixing introduced formatting issues, best to just do a quick format.
	cargo fmt --all

.PHONY: update-dsl-schema-hashes
update-dsl-schema-hashes:  ## Update the DSL schema hashes file
	cargo run --all-features --bin dsl-schema update-hashes

.PHONY: pre-commit
pre-commit: fmt clippy clippy-default  ## Run all code quality checks

.PHONY: clean
clean:  ## Clean up caches, build artifacts, and the venv
	@$(MAKE) -s -C py-polars/ $@
	@rm -rf .ruff_cache/
	@rm -rf .hypothesis/
	@rm -rf .venv/
	@cargo clean

# ============================================================================
# Docker Commands (for use with rust-arblab or standalone)
# ============================================================================

.PHONY: rebuild
rebuild:  ## Clean and rebuild Polaroid gRPC server
	@echo "🔨 Rebuilding Polaroid gRPC server..."
	cd polaroid-grpc && cargo clean && cargo build --release
	@echo "✅ Rebuild complete!"

.PHONY: docker-build
docker-build:  ## Build Polaroid gRPC Docker image
	@echo "🐳 Building Polaroid Docker image..."
	@if [ -n "$(ARBLAB_DIR)" ]; then \
		echo "Building from rust-arblab context..."; \
		cd $(ARBLAB_DIR) && DOCKER_BUILDKIT=1 docker compose build polaroid; \
	elif [ -f "Dockerfile" ]; then \
		echo "Building standalone..."; \
		DOCKER_BUILDKIT=1 docker build -t polaroid-grpc:latest .; \
	else \
		echo "❌ Error: No Dockerfile found and not in rust-arblab context"; \
		exit 1; \
	fi
	@echo "✅ Docker image built!"

.PHONY: docker-up
docker-up:  ## Start Polaroid gRPC server
	@echo "🚀 Starting Polaroid gRPC server..."
	@if [ -n "$(ARBLAB_DIR)" ]; then \
		cd $(ARBLAB_DIR) && docker compose up polaroid; \
	else \
		echo "ℹ️  Running standalone mode"; \
		docker run -p 50052:50052 polaroid-grpc:latest; \
	fi

.PHONY: docker-up-d
docker-up-d:  ## Start Polaroid gRPC server in background
	@echo "🚀 Starting Polaroid gRPC server (detached)..."
	@if [ -n "$(ARBLAB_DIR)" ]; then \
		cd $(ARBLAB_DIR) && docker compose up -d polaroid; \
		echo "✅ Polaroid running at localhost:50052"; \
	else \
		docker run -d -p 50052:50052 --name polaroid-server polaroid-grpc:latest; \
		echo "✅ Polaroid running at localhost:50052"; \
	fi

.PHONY: docker-down
docker-down:  ## Stop Polaroid services
	@echo "🛑 Stopping Polaroid services..."
	@if [ -n "$(ARBLAB_DIR)" ]; then \
		cd $(ARBLAB_DIR) && docker compose stop polaroid; \
	else \
		docker stop polaroid-server 2>/dev/null || true; \
		docker rm polaroid-server 2>/dev/null || true; \
	fi
	@echo "✅ Polaroid stopped"

.PHONY: docker-logs
docker-logs:  ## View Polaroid container logs
	@echo "📋 Tailing Polaroid logs (Ctrl+C to exit)..."
	@if [ -n "$(ARBLAB_DIR)" ]; then \
		cd $(ARBLAB_DIR) && docker compose logs -f polaroid; \
	else \
		docker logs -f polaroid-server; \
	fi

.PHONY: docker-restart
docker-restart:  ## Quick restart without rebuild
	@echo "⚡ Quick restart (no rebuild)..."
	@if [ -n "$(ARBLAB_DIR)" ]; then \
		cd $(ARBLAB_DIR) && docker compose restart polaroid; \
	else \
		docker restart polaroid-server; \
	fi
	@echo "✅ Polaroid restarted!"

.PHONY: docker-rebuild
docker-rebuild: docker-down docker-build docker-up-d  ## Full rebuild and restart
	@echo "✅ Full rebuild complete! Polaroid running at localhost:50052"

.PHONY: docker-clean
docker-clean:  ## Remove all Docker artifacts and rebuild
	@echo "🧹 Cleaning Docker artifacts..."
	@if [ -n "$(ARBLAB_DIR)" ]; then \
		cd $(ARBLAB_DIR) && docker compose down polaroid --remove-orphans; \
	else \
		docker stop polaroid-server 2>/dev/null || true; \
		docker rm polaroid-server 2>/dev/null || true; \
	fi
	docker rmi polaroid-grpc:latest 2>/dev/null || true
	docker system prune -f
	@echo "✅ Docker cleanup complete!"

.PHONY: test
test:  ## Run Polaroid gRPC tests
	cd polaroid-grpc && cargo test --all-features

.PHONY: help
help:  ## Display this help screen
	@echo -e "\033[1m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\033[0m"
	@echo -e "\033[1m  Polaroid - DataFrame Engine Commands\033[0m"
	@echo -e "\033[1m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\033[0m"
	@echo
	@echo "🚀 Quick Start:"
	@echo "  make requirements       Set up Python virtual environment"
	@echo "  make build-release      Build Polaroid (optimized)"
	@echo "  make docker-build       Build Polaroid Docker image"
	@echo "  make docker-up          Start Polaroid gRPC server"
	@echo
	@echo "🔨 Build Commands:"
	@grep -E '^(build|build-release|build-debug-release):.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "🐳 Docker Commands:"
	@echo "  make docker-build       Build Polaroid gRPC Docker image"
	@echo "  make docker-up          Start Polaroid gRPC server (port 50052)"
	@echo "  make docker-up-d        Start in background (detached)"
	@echo "  make docker-down        Stop Polaroid services"
	@echo "  make docker-logs        View container logs"
	@echo "  make docker-restart     Quick restart (no rebuild)"
	@echo "  make docker-rebuild     Full rebuild and restart"
	@echo "  make docker-clean       Remove all Docker artifacts"
	@echo
	@echo "🧪 Rust Commands:"
	@grep -E '^(check|clippy|test|fmt):.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "🧹 Maintenance:"
	@grep -E '^(clean|rebuild):.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "💡 Tips:"
	@echo "  - Use LTS_CPU=1 for older CPU compatibility"
	@echo "  - Use ARGS=\"--no-default-features\" for custom builds"
	@echo "  - Docker context optimized (99.87% smaller with .dockerignore)"
	@echo

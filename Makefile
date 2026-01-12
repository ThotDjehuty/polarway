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
# Docker Commands (optimized builds with caching)
# ============================================================================

.PHONY: docker-build-fast
docker-build-fast:  ## Build Docker image with cargo-chef caching (FAST)
	@echo "🚀 Building Polaroid Docker image (optimized with cargo-chef)..."
	@DOCKER_BUILDKIT=1 docker compose -f docker-compose.polaroid.yml build \
		--build-arg BUILDKIT_INLINE_CACHE=1 \
		--progress=plain
	@echo "✅ Docker image built with dependency caching!"

.PHONY: docker-build
docker-build: docker-build-fast  ## Alias for docker-build-fast

.PHONY: docker-build-clean
docker-build-clean:  ## Build Docker image from scratch (no cache)
	@echo "🧹 Building Polaroid Docker image (clean build, no cache)..."
	@DOCKER_BUILDKIT=1 docker compose -f docker-compose.polaroid.yml build --no-cache --progress=plain
	@echo "✅ Clean Docker build complete!"

.PHONY: docker-up
docker-up:  ## Start Polaroid gRPC server (with volume mounts)
	@echo "🚀 Starting Polaroid gRPC server..."
	@docker compose -f docker-compose.polaroid.yml up
	
.PHONY: docker-up-d
docker-up-d:  ## Start Polaroid gRPC server in background
	@echo "🚀 Starting Polaroid gRPC server (detached)..."
	@docker compose -f docker-compose.polaroid.yml up -d
	@echo "✅ Polaroid running at localhost:50053"
	@echo "📂 Volume mounts:"
	@echo "   - /tmp -> /tmp (test data)"
	@echo "   - ./data -> /app/data"
	@echo "   - ./notebooks -> /app/notebooks (read-only)"

.PHONY: docker-down
docker-down:  ## Stop Polaroid services
	@echo "🛑 Stopping Polaroid services..."
	@docker compose -f docker-compose.polaroid.yml down
	@echo "✅ Polaroid stopped"

.PHONY: docker-logs
docker-logs:  ## View Polaroid container logs
	@echo "📋 Tailing Polaroid logs (Ctrl+C to exit)..."
	@docker compose -f docker-compose.polaroid.yml logs -f

.PHONY: docker-restart
docker-restart:  ## Quick restart without rebuild
	@echo "⚡ Quick restart (no rebuild)..."
	@docker compose -f docker-compose.polaroid.yml restart
	@echo "✅ Polaroid restarted!"

.PHONY: docker-rebuild
docker-rebuild: docker-down docker-build-fast docker-up-d  ## Full rebuild and restart (FAST)
	@echo "✅ Full rebuild complete! Polaroid running at localhost:50053"

.PHONY: docker-clean
docker-clean:  ## Remove all Docker artifacts
	@echo "🧹 Cleaning Docker artifacts..."
	@docker compose -f docker-compose.polaroid.yml down --remove-orphans --volumes
	@docker rmi polaroid-grpc:latest 2>/dev/null || true
	@docker builder prune -f
	@echo "✅ Docker cleanup complete!"

.PHONY: docker-shell
docker-shell:  ## Open shell in running Polaroid container
	@docker compose -f docker-compose.polaroid.yml exec polaroid-grpc /bin/bash || echo "Container not running. Start with: make docker-up-d"

.PHONY: docker-stats
docker-stats:  ## Show Docker container resource usage
	@docker stats polaroid-server-public --no-stream

.PHONY: rebuild
rebuild:  ## Clean and rebuild Polaroid gRPC server locally
	@echo "🔨 Rebuilding Polaroid gRPC server..."
	cd polaroid-grpc && cargo clean && cargo build --release
	@echo "✅ Rebuild complete!"

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
	@echo "  make requirements           Set up Python virtual environment"
	@echo "  make build-release          Build Polaroid (optimized)"
	@echo "  make docker-build-fast      Build Docker image (with caching - FAST!)"
	@echo "  make docker-up-d            Start Polaroid gRPC server"
	@echo
	@echo "🔨 Build Commands:"
	@grep -E '^(build|build-release|build-debug-release):.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "🐳 Docker Commands (Optimized!):"
	@echo "  make docker-build-fast      Build with cargo-chef caching (2nd build is ~90% faster!)"
	@echo "  make docker-build-clean     Clean build without cache"
	@echo "  make docker-up              Start gRPC server (foreground)"
	@echo "  make docker-up-d            Start in background (port 50053)"
	@echo "  make docker-down            Stop Polaroid services"
	@echo "  make docker-logs            View container logs (follow)"
	@echo "  make docker-restart         Quick restart (no rebuild)"
	@echo "  make docker-rebuild         Full rebuild and restart (FAST)"
	@echo "  make docker-clean           Remove all Docker artifacts"
	@echo "  make docker-shell           Open bash shell in container"
	@echo "  make docker-stats           Show resource usage"
	@echo
	@echo "🧪 Rust Commands:"
	@grep -E '^(check|clippy|test|fmt):.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "🧹 Maintenance:"
	@grep -E '^(clean|rebuild):.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "💡 Performance Tips:"
	@echo "  - First Docker build: ~3-5 minutes (compiles all dependencies)"
	@echo "  - Subsequent builds: ~30-60 seconds (only app code, deps cached!)"
	@echo "  - Use 'docker-build-fast' instead of 'docker-build'"
	@echo "  - Volume mounts: /tmp, ./data, ./notebooks (for Jupyter access)"
	@echo "  - Docker context: 99.87% smaller with .dockerignore"
	@echo

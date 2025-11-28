# Roxidy - Tauri Terminal App
# Run `just` to see all available commands

# Default recipe - show help
default:
    @just --list

# ============================================
# Development
# ============================================

# Start development server (frontend + backend)
dev:
    pnpm tauri dev

# Start only the frontend dev server
dev-fe:
    pnpm dev

# ============================================
# Testing
# ============================================

# Run all tests (frontend + backend)
test: test-fe test-rust

# Run frontend tests
test-fe:
    pnpm test:run

# Run frontend tests in watch mode
test-watch:
    pnpm test

# Run frontend tests with UI
test-ui:
    pnpm test:ui

# Run frontend tests with coverage
test-coverage:
    pnpm test:coverage

# Run Rust tests
test-rust:
    cd src-tauri && cargo test

# Run Rust tests with output
test-rust-verbose:
    cd src-tauri && cargo test -- --nocapture

# ============================================
# Building
# ============================================

# Build for production
build:
    pnpm tauri build

# Build frontend only
build-fe:
    pnpm build

# Build Rust backend only (debug)
build-rust:
    cd src-tauri && cargo build

# Build Rust backend (release)
build-rust-release:
    cd src-tauri && cargo build --release

# ============================================
# Code Quality
# ============================================

# Run all checks (lint, format, typecheck)
check: check-fe check-rust

# Check frontend (biome)
check-fe:
    pnpm check

# Check Rust (clippy + fmt check)
check-rust:
    cd src-tauri && cargo clippy -- -D warnings
    cd src-tauri && cargo fmt --check

# Fix frontend issues (biome)
fix:
    pnpm check:fix

# Format all code
fmt: fmt-fe fmt-rust

# Format frontend
fmt-fe:
    pnpm format

# Format Rust
fmt-rust:
    cd src-tauri && cargo fmt

# Lint frontend
lint:
    pnpm lint

# Lint and fix frontend
lint-fix:
    pnpm lint:fix

# ============================================
# Cleaning
# ============================================

# Clean all build artifacts
clean: clean-fe clean-rust

# Clean frontend
clean-fe:
    rm -rf dist node_modules/.vite

# Clean Rust
clean-rust:
    cd src-tauri && cargo clean

# Deep clean (includes node_modules)
clean-all: clean
    rm -rf node_modules

# ============================================
# Dependencies
# ============================================

# Install all dependencies
install:
    pnpm install

# Update frontend dependencies
update-fe:
    pnpm update

# Update Rust dependencies
update-rust:
    cd src-tauri && cargo update

# ============================================
# Utilities
# ============================================

# Kill any running dev processes
kill:
    -pkill -f "target/debug/roxidy" 2>/dev/null
    -pkill -f "vite" 2>/dev/null
    -lsof -ti:1420 | xargs kill -9 2>/dev/null

# Restart dev (kill + dev)
restart: kill dev

# Show Rust dependency tree
deps:
    cd src-tauri && cargo tree

# Open Rust docs
docs:
    cd src-tauri && cargo doc --open

# Run a quick sanity check before committing
precommit: check test
    @echo "✓ All checks passed!"

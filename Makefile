.PHONY: help build test clippy fmt clean run dev release screenshot diagrams all check

help:
	@echo "speedsys-rs — Development commands"
	@echo ""
	@echo "Build:"
	@echo "  make build          Build debug binary"
	@echo "  make release        Build optimized release binary"
	@echo "  make clean          Clean build artifacts"
	@echo ""
	@echo "Development:"
	@echo "  make dev            Build + run in debug mode"
	@echo "  make run            Run release binary (requires sudo for disk access)"
	@echo "  make check          Run all quality checks"
	@echo ""
	@echo "Quality:"
	@echo "  make test           Run all tests (unit + integration)"
	@echo "  make clippy         Run clippy linter (must pass)"
	@echo "  make fmt            Check code formatting"
	@echo "  make fmt-fix        Auto-fix formatting"
	@echo ""
	@echo "Documentation:"
	@echo "  make diagrams       Regenerate Graphviz diagrams"
	@echo "  make screenshot     Generate new screenshots"
	@echo "  make docs           Build all docs/diagrams/screenshots"
	@echo ""
	@echo "Meta:"
	@echo "  make all            Build + test + clippy (pre-commit checklist)"
	@echo "  make help           Show this message"

build:
	cargo build

release:
	cargo build --release

clean:
	cargo clean
	rm -f docs/diagrams/*.{svg,png}
	rm -f docs/screenshots/*.svg

dev:
	cargo run --bin speedsys-rs

run: release
	sudo ./target/release/speedsys-rs

check: test clippy fmt
	@echo "✓ All checks passed"

test:
	cargo test --lib
	cargo test --test integration_tests

clippy:
	cargo clippy -- -D warnings

fmt:
	rustfmt --check src/**/*.rs

fmt-fix:
	rustfmt src/**/*.rs

diagrams: release
	./scripts/render_diagrams.sh

screenshot: release
	mkdir -p docs/screenshots
	./target/release/speedsys-rs --screenshot overview --screenshot-out docs/screenshots/overview.svg
	./target/release/speedsys-rs --screenshot disk-select --screenshot-out docs/screenshots/disk-select.svg
	./target/release/speedsys-rs --screenshot disk-test --screenshot-out docs/screenshots/disk-test.svg
	@echo "✓ Screenshots generated (SVG). Converting to PNG..."
	for f in docs/screenshots/*.svg; do rsvg-convert -o "$${f%.svg}.png" "$$f"; done
	@echo "✓ PNG conversion complete"

docs: diagrams screenshot
	@echo "✓ All documentation regenerated"

all: build test clippy fmt
	@echo ""
	@echo "╔════════════════════════════════════════╗"
	@echo "║  ✓ Build successful                    ║"
	@echo "║  ✓ Tests passing                       ║"
	@echo "║  ✓ Clippy warnings: 0                  ║"
	@echo "║  ✓ Code formatting check               ║"
	@echo "║                                        ║"
	@echo "║  Ready for commit!                     ║"
	@echo "╚════════════════════════════════════════╝"

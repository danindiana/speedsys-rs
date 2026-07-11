#!/bin/bash
# Regenerate all screenshots
set -e

cd "$(dirname "$0")/.."

echo "Building release binary..."
cargo build --release

echo "Generating screenshots..."
mkdir -p docs/screenshots

./target/release/speedsys-rs --screenshot overview --screenshot-out docs/screenshots/overview.svg
./target/release/speedsys-rs --screenshot disk-select --screenshot-out docs/screenshots/disk-select.svg
./target/release/speedsys-rs --screenshot disk-test --screenshot-out docs/screenshots/disk-test.svg

echo "Converting to PNG..."
for f in docs/screenshots/*.svg; do
    rsvg-convert -o "${f%.svg}.png" "$f"
done

echo "✓ Screenshots regenerated"
ls -lh docs/screenshots/ | tail -6

#!/bin/bash
# Render all Graphviz diagrams to SVG and PNG
set -e

cd "$(dirname "$0")/../docs/diagrams"

for dotfile in *.dot; do
    name="${dotfile%.dot}"
    echo "Rendering $name..."
    dot -Tsvg "$dotfile" -o "$name.svg"
    dot -Tpng "$dotfile" -o "$name.png"
done

echo "✓ All diagrams rendered"
ls -lh *.svg *.png | grep -v "^total"

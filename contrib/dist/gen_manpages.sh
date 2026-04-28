#!/usr/bin/env bash
# Convert all markdown files in /doc/rpc/ to man pages in /doc/rpc_man/
# Must have pandoc installed

set -euo pipefail

check_installed() {
    if ! command -v "$1" &>/dev/null; then
        echo "You must have $1 installed to run this script!"
        exit 1
    fi
}

check_dependencies() {
    check_installed "pandoc"
    check_installed "gzip"
}

# Get the script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SOURCE_DIR="$PROJECT_ROOT/doc/rpc"
TARGET_DIR="$PROJECT_ROOT/doc/rpc_man"

# Function to convert a single markdown file to man page
convert_single() {
    local file="$1"
    if [[ ! -f "$file" ]]; then
        echo "Error: File $file not found"
        exit 1
    fi

    # Extract filename without extension
    local basename
    basename=$(basename "$file" .md)

    # Skip the template stub file
    if [[ "$basename" == "template" ]]; then
        echo "Skipping template file: $file"
        return 0
    fi

    # Create man page directory if it doesn't exist
    mkdir -p "$TARGET_DIR"

    # Convert markdown to man page using pandoc
    # Default to section 1 (user commands) unless specified in filename
    local section=1
    if [[ "$basename" =~ \.[0-9]$ ]]; then
        section="${basename##*.}"
        basename="${basename%.*}"
    fi

    # Convert to man page format
    pandoc "$file" \
        -s \
        -t man \
        --metadata title="$basename" \
        --metadata section="$section" \
        --metadata date="$(date +'%B %Y')" \
        -o "$TARGET_DIR/${basename}.${section}"

    # Compress the man page
    gzip -f "$TARGET_DIR/${basename}.${section}"

    echo "Created: $TARGET_DIR/${basename}.${section}.gz"
}

check_dependencies

# Main logic
if [[ $# -eq 1 ]]; then
    # Convert single file if argument provided
    # Handle both absolute and relative paths
    if [[ "$1" = /* ]]; then
        # Absolute path
        convert_single "$1"
    else
        # Relative path - check if it exists as-is, otherwise try in SOURCE_DIR
        if [[ -f "$1" ]]; then
            convert_single "$1"
        elif [[ -f "$SOURCE_DIR/$1" ]]; then
            convert_single "$SOURCE_DIR/$1"
        else
            echo "Error: File $1 not found"
            exit 1
        fi
    fi
else
    # Convert all markdown files in doc/rpc/
    echo "Converting all markdown files in $SOURCE_DIR to man pages..."

    # Check if source directory exists
    if [[ ! -d "$SOURCE_DIR" ]]; then
        echo "Error: Source directory $SOURCE_DIR not found"
        exit 1
    fi

    # Check if there are any .md files
    if ! ls "$SOURCE_DIR"/*.md &>/dev/null; then
        echo "No markdown files found in $SOURCE_DIR"
        exit 1
    fi

    # Convert each .md file to man page
    for md_file in "$SOURCE_DIR"/*.md; do
        if [[ -f "$md_file" ]]; then
            echo "Converting $md_file..."
            convert_single "$md_file"
        fi
    done

    echo "All markdown files converted to man pages in $TARGET_DIR/"
fi

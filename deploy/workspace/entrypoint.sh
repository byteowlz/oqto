#!/bin/sh
set -eu

mkdir -p "$HOME/.config/oqto" "$HOME/.local/share/oqto" /run/oqto

# The placement supervisor owns endpoint selection and passes runner arguments.
# Never synthesize a second control path in the image.
exec "$@"

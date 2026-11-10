#!/bin/sh
# Run the storybook's golden test inside the linux-check container.
#
#   container-compose -f .stacks/linux-check/stack.toml run -T check -- \
#       /bin/sh /work/code/defrag/shared-crates/.stacks/linux-check/visual-linux.sh
#
# Pass `bless` after the script to write the Linux goldens instead of checking
# them.
set -eu

apt-get update -qq
apt-get install -y -qq --no-install-recommends \
    libx11-6 libgl1 libglx-mesa0 libegl1 libgl1-mesa-dri

export CARGO_TERM_COLOR=never
export CARGO_HOME=/work/code/defrag/shared-crates/.tmp/linux-check/cargo-home
export CARGO_TARGET_DIR=/work/code/defrag/shared-crates/.tmp/linux-check/out/target
export LIBGL_ALWAYS_SOFTWARE=1

if [ "${1:-}" = "bless" ]; then
    export UPDATE_GOLDENS=1
fi

cd /work/code/defrag/shared-crates
cargo test -p macroquad-storybook --test visual

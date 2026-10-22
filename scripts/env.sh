set -euo pipefail
FORK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$FORK_ROOT/depot_tools:$PATH"
cd "$FORK_ROOT"

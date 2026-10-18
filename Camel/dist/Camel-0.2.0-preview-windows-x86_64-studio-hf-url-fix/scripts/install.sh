#!/bin/sh
set -eu
bundle_dir=$(CDPATH= cd -P "$(dirname "$0")" && pwd)
chmod u+x "$bundle_dir/camel-setup"
exec "$bundle_dir/camel-setup" --bundle "$bundle_dir"

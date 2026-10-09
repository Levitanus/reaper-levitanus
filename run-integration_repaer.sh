```bash
#!/usr/bin/env bash

set -e

echo "==> Building Rust workspace..."
cargo build --all

echo "==> Copying REAPER plugin..."
cp \
    ./target/debug/libreaper_test_extension_plugin.so \
    ./target/reaper/reaper_linux_x86_64/REAPER/UserPlugins/reaper_test_extension_plugin.so

echo "==> Starting REAPER..."
exec ./target/reaper/reaper_linux_x86_64/REAPER/reaper
```

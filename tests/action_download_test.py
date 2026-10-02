from pathlib import Path


action = Path("action.yml").read_text()

assert 'VERSION="v0.2.1"' in action
assert 'ARCHIVE_PATH="$RUNNER_TEMP/${ARCHIVE}.zip"' in action
assert 'curl -fsSL "${BASE_URL}/${ARCHIVE}.zip" -o "$ARCHIVE_PATH"' in action
assert 'unzip -o "$ARCHIVE_PATH" -d "$RUNNER_TEMP"' in action
assert 'ARCHIVE_PATH="$RUNNER_TEMP/${ARCHIVE}.tar.gz"' in action
assert 'curl -fsSL "${BASE_URL}/${ARCHIVE}.tar.gz" -o "$ARCHIVE_PATH"' in action
assert 'tar -xzf "$ARCHIVE_PATH" -C "$RUNNER_TEMP"' in action
assert action.count('rm -f "$ARCHIVE_PATH"') == 2
assert '-o archive.zip' not in action
assert '-o archive.tar.gz' not in action

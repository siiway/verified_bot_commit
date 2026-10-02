import os
import subprocess
import tarfile
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ACTION = ROOT / "action.yml"


def download_script():
    lines = ACTION.read_text().splitlines()
    start = lines.index("  - name: Download verified-bot-commit")
    run = lines.index("    run: |", start) + 1
    end = lines.index("  - name: Run verified-bot-commit", run)
    return "\n".join(line[6:] for line in lines[run:end])


def write_mock_curl(path):
    path.write_text(
        "#!/bin/sh\n"
        "if [ \"$MOCK_CURL_FAILURE\" = \"1\" ]; then\n"
        "  : > \"$4\"\n"
        "  exit 1\n"
        "fi\n"
        "cp \"$MOCK_ARCHIVE\" \"$4\"\n"
    )
    path.chmod(0o755)


def run_download(workspace, runner_temp, archive, fail=False):
    bin_dir = workspace / "bin"
    bin_dir.mkdir()
    write_mock_curl(bin_dir / "curl")
    env = os.environ | {
        "GITHUB_ENV": str(runner_temp / "github_env"),
        "MOCK_ARCHIVE": str(archive),
        "MOCK_CURL_FAILURE": "1" if fail else "0",
        "PATH": f"{bin_dir}:{os.environ['PATH']}",
        "RUNNER_ARCH": "X64",
        "RUNNER_OS": "Linux",
        "RUNNER_TEMP": str(runner_temp),
    }
    return subprocess.run(
        ["bash", "-e", "-c", download_script()],
        cwd=workspace,
        env=env,
        check=False,
    )


with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    workspace = root / "workspace"
    runner_temp = root / "runner_temp"
    workspace.mkdir()
    runner_temp.mkdir()
    binary = root / "verified_bot_commit"
    binary.write_text("test binary")
    archive = root / "release.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        tar.add(binary, arcname="verified_bot_commit")

    assert run_download(workspace, runner_temp, archive).returncode == 0
    assert (runner_temp / "verified_bot_commit").read_text() == "test binary"
    assert not (runner_temp / "verified_bot_commit-x86_64-unknown-linux-gnu.tar.gz").exists()
    assert not (workspace / "archive.tar.gz").exists()

    failed_workspace = root / "failed_workspace"
    failed_temp = root / "failed_temp"
    failed_workspace.mkdir()
    failed_temp.mkdir()
    assert run_download(failed_workspace, failed_temp, archive, fail=True).returncode != 0
    assert not (failed_temp / "verified_bot_commit-x86_64-unknown-linux-gnu.tar.gz").exists()

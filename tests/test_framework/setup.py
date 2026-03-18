"""
test_framework.setup

Binary preparation and environment setup for functional tests.

Handles building/downloading florestad, utreexod, and bitcoind,
and resolving the FLORESTA_TEMP_DIR used by all test infrastructure.
"""

import hashlib
import os
import platform
import shutil
import subprocess
import tarfile
import tempfile
import urllib.error
import urllib.request


def check_installed(cmd: str):
    """Verify that *cmd* is on PATH; raise if not."""
    if shutil.which(cmd) is None:
        raise RuntimeError(f"Required command '{cmd}' is not installed.")


def _git_describe() -> str:
    """Return `git describe --tags --always` from the project root."""
    proj_dir = get_project_dir()
    result = subprocess.run(
        ["git", "describe", "--tags", "--always"],
        capture_output=True,
        text=True,
        cwd=proj_dir,
        check=True,
    )
    return result.stdout.strip()


def get_project_dir() -> str:
    """Return the root of the git repository."""
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout.strip()


def get_temp_dir() -> str:
    """
    Resolve FLORESTA_TEMP_DIR.

    If the env var is already set, use it. Otherwise compute a deterministic
    path from ``git describe`` and export it so child processes inherit it.
    """
    temp_dir = os.environ.get("FLORESTA_TEMP_DIR")
    if temp_dir:
        os.makedirs(os.path.join(temp_dir, "binaries"), exist_ok=True)
        return temp_dir

    temp_dir = "/tmp/floresta-func-tests"
    os.makedirs(os.path.join(temp_dir, "binaries"), exist_ok=True)
    os.environ["FLORESTA_TEMP_DIR"] = temp_dir
    return temp_dir


# ---------------------------------------------------------------------------
# Shared helpers
# ---------------------------------------------------------------------------


def _sha256_file(path: str) -> str:
    """Return the hex SHA256 digest of a file."""
    sha256 = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            sha256.update(chunk)
    return sha256.hexdigest()


def _lookup_manifest_hash(manifest_path: str, target_name: str) -> str | None:
    """Look up *target_name* in a ``<hash>  <filename>`` manifest file."""
    with open(manifest_path, "r", encoding="utf-8") as fh:
        for line in fh:
            parts = line.strip().split()
            if len(parts) == 2 and parts[1] == target_name:
                return parts[0]
    return None


def _download_file(url: str, dest: str) -> bool:
    """Download *url* to *dest*. Returns False on network errors."""
    print(f"Downloading {url}...")
    try:
        urllib.request.urlretrieve(url, dest)
        return True
    except (urllib.error.URLError, OSError) as exc:
        print(f"Download failed: {exc}")
        return False


def _verify_and_extract(
    tarball_path: str, expected_hash: str, extract_dir: str
) -> bool:
    """Verify SHA256 of *tarball_path* and extract into *extract_dir*."""
    actual_hash = _sha256_file(tarball_path)
    tarball_name = os.path.basename(tarball_path)
    if actual_hash != expected_hash.strip():
        print(
            f"SHA256 mismatch for {tarball_name}\n"
            f"Expected: {expected_hash}\n"
            f"Actual:   {actual_hash}"
        )
        return False
    with tarfile.open(tarball_path, "r:gz") as tar:
        tar.extractall(path=extract_dir)
    return True


def _copy_executable(src: str, binaries_dir: str, name: str):
    """Copy *src* into *binaries_dir*/*name* and make it executable."""
    dst = os.path.join(binaries_dir, name)
    shutil.copy2(src, dst)
    os.chmod(dst, 0o755)


def _use_env_executable(binaries_dir: str, env_var: str, binary_name: str) -> bool:
    """If *env_var* is set, copy the executable into *binaries_dir*."""
    exe = os.environ.get(env_var)
    if not exe:
        return False
    if not os.path.isfile(exe) or not os.access(exe, os.X_OK):
        raise RuntimeError(
            f"{env_var} is set but does not point to an executable: {exe}"
        )
    _copy_executable(exe, binaries_dir, binary_name)
    print(
        f"Using user-provided {binary_name}: {os.path.join(binaries_dir, binary_name)}"
    )
    return True


# ---------------------------------------------------------------------------
# florestad
# ---------------------------------------------------------------------------


def build_florestad(binaries_dir: str, release: bool = False):
    """Build florestad via cargo and symlink into *binaries_dir*."""
    check_installed("cargo")

    proj_dir = get_project_dir()
    cmd = ["cargo", "build", "--bin", "florestad"]
    if release:
        cmd.append("--release")
        profile = "release"
    else:
        profile = "debug"

    print(f"Building florestad ({profile})...")
    subprocess.run(cmd, cwd=proj_dir, check=True)

    src = os.path.join(proj_dir, "target", profile, "florestad")
    dst = os.path.join(binaries_dir, "florestad")

    # Remove existing link/file so ln -sf semantics work
    if os.path.lexists(dst):
        os.remove(dst)
    os.symlink(src, dst)


# ---------------------------------------------------------------------------
# utreexod
# ---------------------------------------------------------------------------

DEFAULT_UTREEXOD_REVISION = "878794f30cf0ffd499bc03551186ce6a5c16b67e"

_UTREEXOD_PLATFORM_MAP = {
    ("Linux", "x86_64"): "linux-amd64",
    ("Linux", "aarch64"): "linux-arm64",
    ("Linux", "arm64"): "linux-arm64",
    ("Linux", "armv7l"): "linux-armv7",
    ("Linux", "armv6l"): "linux-armv6",
    ("Darwin", "x86_64"): "darwin-amd64",
    ("Darwin", "arm64"): "darwin-arm64",
    ("Darwin", "aarch64"): "darwin-arm64",
}


def _lookup_hash_by_platform(
    hash_file: str, platform_substr: str
) -> tuple[str, str] | None:
    """Find the (hash, filename) entry in *hash_file* whose filename contains *platform_substr*."""
    with open(hash_file, "r", encoding="utf-8") as fh:
        for line in fh:
            parts = line.strip().split()
            if len(parts) == 2 and platform_substr in parts[1]:
                return parts[0], parts[1]
    return None


def _download_prebuilt_utreexod(binaries_dir: str, revision: str) -> bool:
    """Download a prebuilt utreexod, verify SHA256 against local hash file."""
    proj_dir = get_project_dir()
    hash_file = os.path.join(proj_dir, "tests", "utreexod_hashes", revision)
    if not os.path.isfile(hash_file):
        return False

    plat = _UTREEXOD_PLATFORM_MAP.get((platform.system(), platform.machine()))
    if plat is None:
        return False

    result = _lookup_hash_by_platform(hash_file, f"utreexod-{plat}-")
    if result is None:
        return False

    expected_hash, tarball_name = result
    url = f"https://github.com/utreexo/utreexod/releases/download/{revision}/{tarball_name}"

    with tempfile.TemporaryDirectory() as tmp:
        tarball_path = os.path.join(tmp, tarball_name)
        if not _download_file(url, tarball_path):
            return False

        if not _verify_and_extract(tarball_path, expected_hash, tmp):
            return False

        extracted_dir = tarball_name.replace(".tar.gz", "")
        src = os.path.join(tmp, extracted_dir, "utreexod")
        _copy_executable(src, binaries_dir, "utreexod")

    print(f"utreexod downloaded to {binaries_dir}/utreexod")
    return True


def _build_utreexod_from_source(
    binaries_dir: str,
    revision: str = DEFAULT_UTREEXOD_REVISION,
) -> bool:
    """Clone, checkout *revision*, and build utreexod into *binaries_dir*."""
    for cmd in ("git", "go"):
        if shutil.which(cmd) is None:
            return False

    with tempfile.TemporaryDirectory() as tmp:
        repo_dir = os.path.join(tmp, "utreexod")
        print("Cloning utreexod...")
        subprocess.run(
            ["git", "clone", "https://github.com/utreexo/utreexod", repo_dir],
            check=True,
        )

        print(f"Checking out utreexod at {revision}...")
        subprocess.run(["git", "checkout", revision], cwd=repo_dir, check=True)

        print("Building utreexod...")
        subprocess.run(
            ["go", "build", "-o", os.path.join(binaries_dir, "utreexod"), "."],
            cwd=repo_dir,
            check=True,
        )

    print("utreexod built successfully.")
    return True


def ensure_utreexod(binaries_dir: str, revision: str = DEFAULT_UTREEXOD_REVISION):
    """Obtain utreexod by any available method."""
    if _use_env_executable(binaries_dir, "UTREEXOD_EXE", "utreexod"):
        return
    if _download_prebuilt_utreexod(binaries_dir, revision):
        return
    if _build_utreexod_from_source(binaries_dir, revision=revision):
        return
    raise RuntimeError(
        "Failed to obtain utreexod "
        "(tried UTREEXOD_EXE, prebuilt download, and source build)."
    )


# ---------------------------------------------------------------------------
# bitcoind
# ---------------------------------------------------------------------------

DEFAULT_BITCOIN_REVISION = "30.2"

# Map (uname_system, uname_machine) -> Bitcoin Core platform string
_PLATFORM_MAP = {
    ("Linux", "x86_64"): "x86_64-linux-gnu",
    ("Linux", "aarch64"): "aarch64-linux-gnu",
    ("Linux", "arm64"): "aarch64-linux-gnu",
    ("Darwin", "x86_64"): "x86_64-apple-darwin",
    ("Darwin", "arm64"): "arm64-apple-darwin",
    ("Darwin", "aarch64"): "arm64-apple-darwin",
}


def _download_prebuilt_bitcoind(binaries_dir: str, revision: str) -> bool:
    """Download a prebuilt bitcoind tarball, verify SHA256, and extract."""
    proj_dir = get_project_dir()
    hash_file = os.path.join(proj_dir, "tests", "bitcoin_hashes", revision)
    if not os.path.isfile(hash_file):
        return False

    key = _PLATFORM_MAP.get((platform.system(), platform.machine()))
    if key is None:
        return False

    file_name = f"bitcoin-{revision}-{key}.tar.gz"
    expected_hash = _lookup_manifest_hash(hash_file, file_name)
    if expected_hash is None:
        return False

    url = f"https://bitcoincore.org/bin/bitcoin-core-{revision}/{file_name}"

    with tempfile.TemporaryDirectory() as tmp:
        dl_path = os.path.join(tmp, file_name)
        if not _download_file(url, dl_path):
            return False

        if not _verify_and_extract(dl_path, expected_hash, tmp):
            return False

        src = os.path.join(tmp, f"bitcoin-{revision}", "bin", "bitcoind")
        _copy_executable(src, binaries_dir, "bitcoind")

    print(f"bitcoind downloaded to {binaries_dir}/bitcoind")
    return True


def _build_bitcoind_from_source(binaries_dir: str, revision: str) -> bool:
    """Build bitcoind from source (cmake for v29+)."""
    for cmd in ("git", "make", "cmake"):
        if shutil.which(cmd) is None:
            return False

    # Need a C++ compiler
    if shutil.which("gcc") is None and shutil.which("clang") is None:
        return False

    nprocs = os.environ.get("BUILD_BITCOIND_NPROCS", "4")

    with tempfile.TemporaryDirectory() as tmp:
        repo_dir = os.path.join(tmp, "bitcoin")
        print("Cloning Bitcoin Core...")
        subprocess.run(
            ["git", "clone", "https://github.com/bitcoin/bitcoin", repo_dir],
            check=True,
        )

        # Checkout the requested revision
        tag = f"v{revision}" if not revision.startswith("v") else revision
        subprocess.run(["git", "checkout", tag], cwd=repo_dir, check=True)

        # Determine major version for build system selection
        rev_num = revision.lstrip("v")
        try:
            major = int(rev_num.split(".")[0])
        except ValueError:
            major = 999  # non-numeric branch, assume modern

        if major >= 29:
            subprocess.run(
                [
                    "cmake",
                    "-S",
                    ".",
                    "-B",
                    "build",
                    "-DBUILD_CLI=OFF",
                    "-DBUILD_TESTS=OFF",
                    "-DCMAKE_BUILD_TYPE=MinSizeRel",
                    "-DENABLE_EXTERNAL_SIGNER=OFF",
                    "-DENABLE_IPC=OFF",
                    "-DINSTALL_MAN=OFF",
                ],
                cwd=repo_dir,
                check=True,
            )
            subprocess.run(
                ["cmake", "--build", "build", "--target", "bitcoind", f"-j{nprocs}"],
                cwd=repo_dir,
                check=True,
            )
            src = os.path.join(repo_dir, "build", "bin", "bitcoind")
        else:
            subprocess.run(["./autogen.sh"], cwd=repo_dir, check=True)
            subprocess.run(
                [
                    "./configure",
                    "--without-gui",
                    "--disable-tests",
                    "--disable-bench",
                ],
                cwd=repo_dir,
                check=True,
            )
            subprocess.run(["make", f"-j{nprocs}"], cwd=repo_dir, check=True)
            src = os.path.join(repo_dir, "src", "bitcoind")

        _copy_executable(src, binaries_dir, "bitcoind")

    print(f"bitcoind built from source at {binaries_dir}/bitcoind")
    return True


def ensure_bitcoind(binaries_dir: str, revision: str = DEFAULT_BITCOIN_REVISION):
    """Obtain bitcoind by any available method."""
    if _use_env_executable(binaries_dir, "BITCOIND_EXE", "bitcoind"):
        return
    if _download_prebuilt_bitcoind(binaries_dir, revision):
        return
    if _build_bitcoind_from_source(binaries_dir, revision):
        return
    raise RuntimeError(
        "Failed to obtain bitcoind "
        "(tried BITCOIND_EXE, prebuilt download, and source build)."
    )


def prepare_binaries(release: bool = False, force_rebuild: bool = False):
    """
    Build / download all required binaries.

    Binaries that already exist in the target directory are skipped unless
    *force_rebuild* is ``True``.  This allows external setups (CI caches,
    Nix, manual builds) to pre-populate the binaries directory and avoid
    redundant work.

    Returns the binaries directory path.
    """
    temp_dir = get_temp_dir()
    binaries_dir = os.path.join(temp_dir, "binaries")

    utreexod_rev = os.environ.get("UTREEXOD_REVISION", DEFAULT_UTREEXOD_REVISION)
    bitcoin_rev = os.environ.get("BITCOIN_REVISION", DEFAULT_BITCOIN_REVISION)

    # florestad: build if missing or forced
    florestad_path = os.path.join(binaries_dir, "florestad")
    if not os.path.lexists(florestad_path) or force_rebuild:
        build_florestad(binaries_dir, release=release)
    else:
        print("florestad already present, skipping...")

    # utreexod: obtain if missing or forced
    utreexod_path = os.path.join(binaries_dir, "utreexod")
    if not os.path.isfile(utreexod_path) or force_rebuild:
        ensure_utreexod(binaries_dir, revision=utreexod_rev)
    else:
        print("utreexod already present, skipping...")

    # bitcoind: obtain if missing or forced
    bitcoind_path = os.path.join(binaries_dir, "bitcoind")
    if not os.path.isfile(bitcoind_path) or force_rebuild:
        ensure_bitcoind(binaries_dir, revision=bitcoin_rev)
    else:
        print("bitcoind already present, skipping...")

    print(f"All binaries ready at {binaries_dir}")
    return binaries_dir

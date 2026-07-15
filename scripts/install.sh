#!/usr/bin/env sh
set -eu

usage() {
	cat <<'USAGE'
Usage: install.sh [--version TAG] [--dir DIR]

Download an airgradient-cli release archive from GitHub Releases, verify its
sha256 checksum, and install the `airgradient-cli` binary into a directory on
PATH. If that directory is not already on PATH, add it to ~/.bashrc and
~/.zshrc.

Options:
  --version TAG   Install a specific release tag instead of the latest
                   (e.g. v0.1.0)
  --dir DIR       Install directory (default: $HOME/.local/bin)
  --help          Show this help text

Environment overrides:
  AIRGRADIENT_CLI_INSTALL_VERSION  Same as --version
  AIRGRADIENT_CLI_INSTALL_DIR      Same as --dir

Only Linux amd64/arm64 binaries are published; see docs/release-boundary.md
to build from source on other platforms.
USAGE
}

repo="worxbend/airgradient-cli"
bin_name="airgradient-cli"
version=${AIRGRADIENT_CLI_INSTALL_VERSION:-latest}
install_dir=${AIRGRADIENT_CLI_INSTALL_DIR:-"$HOME/.local/bin"}

while [ "$#" -gt 0 ]; do
	case "$1" in
	--version)
		if [ "$#" -lt 2 ]; then
			echo "missing value for --version" >&2
			exit 2
		fi
		version=$2
		shift 2
		;;
	--dir)
		if [ "$#" -lt 2 ]; then
			echo "missing value for --dir" >&2
			exit 2
		fi
		install_dir=$2
		shift 2
		;;
	--help | -h)
		usage
		exit 0
		;;
	*)
		echo "unknown option: $1" >&2
		usage >&2
		exit 2
		;;
	esac
done

os=$(uname -s)
case "$os" in
Linux) ;;
*)
	echo "airgradient-cli's install script only supports Linux (got $os)." >&2
	echo "Build from source with 'cargo install --path .'; see docs/release-boundary.md." >&2
	exit 1
	;;
esac

arch=$(uname -m)
case "$arch" in
x86_64 | amd64) arch_suffix=amd64 ;;
aarch64 | arm64) arch_suffix=arm64 ;;
*)
	echo "unsupported architecture: $arch (airgradient-cli publishes linux amd64/arm64 only)" >&2
	exit 1
	;;
esac

fetch() {
	url=$1
	out=$2
	if command -v curl >/dev/null 2>&1; then
		curl --proto '=https' --tlsv1.2 -fsSL "$url" -o "$out"
	elif command -v wget >/dev/null 2>&1; then
		wget -q "$url" -O "$out"
	else
		echo "curl or wget is required to install airgradient-cli" >&2
		exit 1
	fi
}

fetch_stdout() {
	url=$1
	if command -v curl >/dev/null 2>&1; then
		curl --proto '=https' --tlsv1.2 -fsSL "$url"
	elif command -v wget >/dev/null 2>&1; then
		wget -q "$url" -O -
	else
		echo "curl or wget is required to install airgradient-cli" >&2
		exit 1
	fi
}

# Release asset filenames embed the version (e.g.
# airgradient-cli-v0.1.0-linux-amd64.tar.gz), so "latest" must first be
# resolved to a concrete tag via the GitHub API before the asset URL can be
# built; unlike a version-independent filename, `.../latest/download/<name>`
# alone cannot work here.
if [ "$version" = "latest" ]; then
	echo "resolving latest release tag..." >&2
	api_response=$(fetch_stdout "https://api.github.com/repos/${repo}/releases/latest")
	version=$(printf '%s' "$api_response" | grep -m1 '"tag_name"' | sed -E 's/.*"tag_name":[[:space:]]*"([^"]+)".*/\1/')
	if [ -z "$version" ]; then
		echo "could not resolve the latest release tag from the GitHub API" >&2
		exit 1
	fi
fi

version_number=${version#v}
asset="${bin_name}-v${version_number}-linux-${arch_suffix}.tar.gz"
base_url="https://github.com/${repo}/releases/download/${version}"

verify_checksum() {
	dir=$1
	file=$2
	(
		cd "$dir"
		if command -v sha256sum >/dev/null 2>&1; then
			grep -F "$file" SHA256SUMS | sha256sum -c -
		elif command -v shasum >/dev/null 2>&1; then
			grep -F "$file" SHA256SUMS | shasum -a 256 -c -
		else
			echo "sha256sum or shasum is required to verify the download" >&2
			exit 1
		fi
	)
}

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/airgradient-cli-install.XXXXXX")
cleanup() {
	rm -rf "$tmp_dir"
}
trap cleanup EXIT HUP INT TERM

echo "downloading $asset ($version)..." >&2
fetch "$base_url/$asset" "$tmp_dir/$asset"
fetch "$base_url/SHA256SUMS" "$tmp_dir/SHA256SUMS"

echo "verifying checksum..." >&2
verify_checksum "$tmp_dir" "$asset"

echo "extracting..." >&2
tar -C "$tmp_dir" -xzf "$tmp_dir/$asset"

mkdir -p "$install_dir"
install -m 0755 "$tmp_dir/$bin_name" "$install_dir/$bin_name"
echo "installed $bin_name to $install_dir/$bin_name" >&2

add_path_line() {
	rc_file=$1
	if grep -qF "$install_dir" "$rc_file" 2>/dev/null; then
		return 0
	fi
	{
		echo ""
		echo "# added by airgradient-cli's install.sh"
		echo "export PATH=\"$install_dir:\$PATH\""
	} >>"$rc_file"
	echo "added $install_dir to PATH in $rc_file" >&2
}

case ":$PATH:" in
*":$install_dir:"*) ;;
*)
	add_path_line "$HOME/.bashrc"
	add_path_line "$HOME/.zshrc"
	echo "PATH updated; restart your shell (or run: export PATH=\"$install_dir:\$PATH\") to use $bin_name" >&2
	;;
esac

"$install_dir/$bin_name" --help >/dev/null
echo "$bin_name installed successfully; run '$bin_name --help' to get started" >&2

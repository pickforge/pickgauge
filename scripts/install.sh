#!/bin/sh
# PickGauge installer: curl -fsSL https://pickforge.dev/pickgauge/install.sh | sh
# Downloads the latest PickGauge desktop bundle from GitHub Releases into your home
# directory. Never uses sudo. Linux (AppImage) and macOS (.app) only.
set -eu

REPO="pickforge/pickgauge"
APP_NAME="PickGauge"
BIN_NAME="pickgauge"
# The window's app_id (bundle identifier). The .desktop basename and
# StartupWMClass must equal it or the running window shows a generic icon.
APP_ID="pickgauge"

# Environment overrides:
#   PICKGAUGE_INSTALL_DIR  Linux AppImage target dir. Default: $HOME/.local/bin.
#   PICKGAUGE_VERSION      Install a specific release tag, such as v0.1.0.
#   GITHUB_TOKEN           Optional token for GitHub API rate limits.

die() {
  printf '%s\n' "$*" >&2
  exit 1
}

preflight() {
  [ -n "${HOME:-}" ] || die "HOME is not set"

  if command -v curl >/dev/null 2>&1; then
    downloader="curl"
  elif command -v wget >/dev/null 2>&1; then
    downloader="wget"
  else
    die "curl or wget is required"
  fi
}

fetch_stdout() {
  fetch_url=$1
  accept="Accept: application/vnd.github+json"

  if [ -z "${GITHUB_TOKEN:-}" ]; then
    if [ "$downloader" = "curl" ]; then
      curl -fsSL -H "$accept" "$fetch_url"
    else
      wget -qO- --header="$accept" "$fetch_url"
    fi
    return
  fi

  # A token is set: never put it in argv (world-readable via `ps`). curl reads
  # its config from stdin, so no file touches disk. wget needs a file, so use a
  # private temp file removed even if the fetch is interrupted.
  if [ "$downloader" = "curl" ]; then
    printf 'header = "Authorization: Bearer %s"\n' "$GITHUB_TOKEN" |
      curl -fsSL -H "$accept" -K - "$fetch_url"
    return
  fi

  auth_conf=$(mktemp "${TMPDIR:-/tmp}/${BIN_NAME}-auth.XXXXXX") ||
    die "could not create a temporary file for the auth header"
  trap 'rm -f "$auth_conf"' EXIT INT TERM
  printf 'header = Authorization: Bearer %s\n' "$GITHUB_TOKEN" > "$auth_conf"
  fetch_status=0
  wget -qO- --config="$auth_conf" --header="$accept" "$fetch_url" || fetch_status=$?
  rm -f "$auth_conf"
  return "$fetch_status"
}

download_to() {
  download_url=$1
  download_dest=$2

  if [ "$downloader" = "curl" ]; then
    curl -fsSL "$download_url" -o "$download_dest"
  else
    wget -qO "$download_dest" "$download_url"
  fi
}

detect_platform() {
  os_name=$(uname -s)
  cpu_arch=$(uname -m)

  case "$os_name" in
    Linux)
      install_kind="appimage"
      bundle_label="appimage"
      ;;
    Darwin)
      install_kind="macapp"
      bundle_label="macOS .app"
      ;;
    *)
      die "The curl installer supports Linux and macOS. Download the Windows installer from https://github.com/${REPO}/releases"
      ;;
  esac

  case "$cpu_arch" in
    x86_64|amd64)
      arch_pattern="(amd64|x86_64|x64|intel)"
      ;;
    aarch64|arm64)
      arch_pattern="(aarch64|arm64|apple-silicon)"
      ;;
    *)
      die "unsupported CPU architecture: $cpu_arch"
      ;;
  esac
}

release_api_url() {
  if [ -n "${PICKGAUGE_VERSION:-}" ]; then
    printf 'https://api.github.com/repos/%s/releases/tags/%s\n' "$REPO" "$PICKGAUGE_VERSION"
  else
    printf 'https://api.github.com/repos/%s/releases/latest\n' "$REPO"
  fi
}

release_ref() {
  if [ -n "${PICKGAUGE_VERSION:-}" ]; then
    printf '%s\n' "$PICKGAUGE_VERSION"
  else
    printf 'latest\n'
  fi
}

resolve_release() {
  api_url=$(release_api_url)
  ref_name=$(release_ref)

  release_json=$(fetch_stdout "$api_url") || die "failed to fetch release metadata for $ref_name. If GitHub API rate limits you, set GITHUB_TOKEN."

  release_tag=$(printf '%s\n' "$release_json" |
    grep -o '"tag_name"[[:space:]]*:[[:space:]]*"[^"]*"' |
    sed -n '1s/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
  [ -n "$release_tag" ] || release_tag=$ref_name

  download_urls=$(printf '%s\n' "$release_json" |
    grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*"' |
    sed 's/.*"\(https[^"]*\)".*/\1/')

  if [ -z "$download_urls" ]; then
    die "no release download assets found for $ref_name. If GitHub API rate limits you, set GITHUB_TOKEN. See https://github.com/${REPO}/releases"
  fi

  asset_url=$(printf '%s\n' "$download_urls" | while IFS= read -r candidate_url; do
    candidate_name=${candidate_url##*/}

    case "$install_kind" in
      appimage)
        case "$candidate_name" in
          *.AppImage) ;;
          *) continue ;;
        esac
        ;;
      macapp)
        case "$candidate_name" in
          *.app.tar.gz) ;;
          *) continue ;;
        esac
        ;;
    esac

    if printf '%s\n' "$candidate_name" | grep -Eiq "$arch_pattern"; then
      printf '%s\n' "$candidate_url"
      break
    fi
  done)

  if [ -z "$asset_url" ]; then
    die "no $bundle_label bundle for $cpu_arch in $ref_name. See https://github.com/${REPO}/releases"
  fi
}

path_must_be_in_home() {
  checked_path=$1

  case "$checked_path" in
    *..*)
      die "install path must not contain '..': $checked_path"
      ;;
  esac
  case "$checked_path" in
    "$HOME"|"$HOME"/*)
      ;;
    *)
      die "install path must be inside HOME: $checked_path"
      ;;
  esac
}

make_tmp_dir() {
  tmp_parent="${TMPDIR:-$HOME/.cache}"

  case "$tmp_parent" in
    "$HOME"|"$HOME"/*)
      ;;
    *)
      tmp_parent="$HOME/.cache"
      ;;
  esac

  mkdir -p "$tmp_parent"
  tmp=$(mktemp -d "$tmp_parent/${BIN_NAME}-install.XXXXXX")
}

download_asset() {
  asset_name=${asset_url##*/}
  asset_path="$tmp/$asset_name"

  download_to "$asset_url" "$asset_path" || die "failed to download $asset_name"
  [ -s "$asset_path" ] || die "downloaded asset is empty: $asset_name"
}

verify_archive_paths() {
  archive_listing="$tmp/archive-listing.txt"

  tar -tzf "$asset_path" > "$archive_listing"
  while IFS= read -r archive_entry; do
    case "$archive_entry" in
      ""|/*|../*|*/../*|..)
        die "archive contains unsafe path: $archive_entry"
        ;;
    esac
  done < "$archive_listing"
}

write_desktop_launcher() {
  launcher_appimage=$1
  launcher_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
  # Basename and StartupWMClass must equal the window's app_id so the desktop
  # environment ties the running window to this entry (and its icon).
  launcher_file="$launcher_dir/$APP_ID.desktop"

  mkdir -p "$launcher_dir" 2>/dev/null || return 0
  {
    printf '[Desktop Entry]\n'
    printf 'Name=%s\n' "$APP_NAME"
    printf 'Exec="%s"\n' "$launcher_appimage"
    printf 'Icon=%s\n' "$APP_ID"
    printf 'StartupWMClass=%s\n' "$APP_ID"
    printf 'Terminal=false\n'
    printf 'Type=Application\n'
    printf 'Categories=Development;\n'
  } > "$launcher_file" 2>/dev/null || return 0
}

path_has_dir() {
  checked_dir=$1

  case ":${PATH:-}:" in
    *:"$checked_dir":*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

install_appimage() {
  install_dir="${PICKGAUGE_INSTALL_DIR:-$HOME/.local/bin}"
  appimage_path="$install_dir/$APP_NAME.AppImage"
  command_path="$install_dir/$BIN_NAME"

  path_must_be_in_home "$install_dir"
  mkdir -p "$install_dir"
  mv "$asset_path" "$appimage_path"
  chmod +x "$appimage_path"
  ln -sf "$appimage_path" "$command_path"
  write_desktop_launcher "$appimage_path" || true

  [ -x "$appimage_path" ] || die "installed AppImage is not executable: $appimage_path"

  printf '%s %s installed to %s.\n' "$APP_NAME" "$release_tag" "$appimage_path"
  if ! path_has_dir "$install_dir"; then
    printf 'Note: %s is not on PATH. Add it to launch with `%s`.\n' "$install_dir" "$BIN_NAME"
  fi
  printf 'Launch with `%s`, `%s`, or from your app menu.\n' "$BIN_NAME" "$appimage_path"
}

install_macapp() {
  applications_dir="$HOME/Applications"
  app_path="$applications_dir/$APP_NAME.app"

  staging="$tmp/extract"

  mkdir -p "$applications_dir" "$staging"
  verify_archive_paths
  # Extract into staging first, then swap: the existing install is destroyed
  # only after a successful extraction, so a failed/partial extract never
  # leaves the user without a working app, and no stale files are merged in.
  tar -xzf "$asset_path" -C "$staging"
  [ -d "$staging/$APP_NAME.app" ] || die "$APP_NAME.app was not found after extracting $asset_name"
  rm -rf "$app_path"
  mv "$staging/$APP_NAME.app" "$app_path"
  xattr -dr com.apple.quarantine "$app_path" 2>/dev/null || true

  printf '%s %s installed to %s.\n' "$APP_NAME" "$release_tag" "$app_path"
  printf 'Open it with: open "%s"\n' "$app_path"
  printf 'If Gatekeeper blocks it, run: xattr -dr com.apple.quarantine "%s"\n' "$app_path"
}

install_asset() {
  if [ "$install_kind" = "appimage" ]; then
    install_appimage
  else
    install_macapp
  fi
}

main() {
  preflight
  detect_platform
  resolve_release
  make_tmp_dir
  trap 'rm -rf "$tmp"' EXIT INT TERM
  download_asset
  install_asset
}

main "$@"

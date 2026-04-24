#!/usr/bin/env bash
# claude-fetch-mcp 설치 (Linux / macOS)
# 사용: curl -fsSL https://raw.githubusercontent.com/Hostingglobal-Tech/claude-fetch-mcp/main/install.sh | bash
#
# 옵션 환경변수:
#   INSTALL_DIR   기본 $HOME/.local/bin
#   VERSION       기본 latest
#   REGISTER_MCP  기본 1 (claude mcp add 자동 실행). 0 이면 수동 등록 필요.

set -euo pipefail

REPO="Hostingglobal-Tech/claude-fetch-mcp"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${VERSION:-latest}"
REGISTER_MCP="${REGISTER_MCP:-1}"

err() { echo "ERROR: $*" >&2; exit 1; }
info() { echo "==> $*"; }

detect_target() {
    local os arch
    case "$(uname -s)" in
        Linux)  os=unknown-linux-musl ;;
        Darwin) os=apple-darwin ;;
        *) err "지원 안 하는 OS: $(uname -s)" ;;
    esac
    case "$(uname -m)" in
        x86_64|amd64)  arch=x86_64 ;;
        arm64|aarch64) arch=aarch64 ;;
        *) err "지원 안 하는 아키텍처: $(uname -m)" ;;
    esac
    echo "${arch}-${os}"
}

TARGET="$(detect_target)"
ASSET="claude-fetch-mcp-${TARGET}.tar.gz"

if [ "$VERSION" = "latest" ]; then
    URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"
else
    URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"
fi

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

info "다운로드: $URL"
if command -v curl >/dev/null; then
    curl -fsSL -o "$TMPDIR/$ASSET" "$URL" || err "다운로드 실패 — 버전/타겟 확인"
elif command -v wget >/dev/null; then
    wget -qO "$TMPDIR/$ASSET" "$URL" || err "다운로드 실패"
else
    err "curl 또는 wget 필요"
fi

info "압축 해제: $TMPDIR"
tar -xzf "$TMPDIR/$ASSET" -C "$TMPDIR"

mkdir -p "$INSTALL_DIR"
mv "$TMPDIR/claude-fetch-mcp" "$INSTALL_DIR/"
chmod 755 "$INSTALL_DIR/claude-fetch-mcp"
info "설치 완료: $INSTALL_DIR/claude-fetch-mcp"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) echo "WARN: $INSTALL_DIR 가 PATH 에 없음. ~/.bashrc 에 추가: export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
esac

if [ "$REGISTER_MCP" = "1" ]; then
    if command -v claude >/dev/null; then
        info "Claude Code MCP 에 등록 (scope=user, name=fetch-rs)"
        claude mcp add --scope user fetch-rs -- "$INSTALL_DIR/claude-fetch-mcp" || {
            echo "WARN: 자동 등록 실패. 수동 등록:"
            echo "  claude mcp add --scope user fetch-rs -- $INSTALL_DIR/claude-fetch-mcp"
        }
        claude mcp list 2>/dev/null | grep -E "fetch-rs" || true
    else
        echo "WARN: claude CLI 미발견. 수동 등록:"
        echo "  claude mcp add --scope user fetch-rs -- $INSTALL_DIR/claude-fetch-mcp"
    fi
fi

info "끝. Claude Code 재시작 후 fetch-rs 도구 사용 가능."

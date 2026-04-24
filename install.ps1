# claude-fetch-mcp 설치 (Windows PowerShell)
# 사용: iwr -useb https://raw.githubusercontent.com/Hostingglobal-Tech/claude-fetch-mcp/main/install.ps1 | iex
#
# 옵션 환경변수:
#   $env:INSTALL_DIR   기본 "$env:LOCALAPPDATA\claude-fetch-mcp"
#   $env:VERSION       기본 latest
#   $env:REGISTER_MCP  기본 1

$ErrorActionPreference = 'Stop'

$Repo = 'Hostingglobal-Tech/claude-fetch-mcp'
$InstallDir = if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'claude-fetch-mcp' }
$Version = if ($env:VERSION) { $env:VERSION } else { 'latest' }
$Register = if ($env:REGISTER_MCP) { $env:REGISTER_MCP } else { '1' }

function Info($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Warn($msg) { Write-Host "WARN: $msg" -ForegroundColor Yellow }

# Detect target
$arch = switch -Wildcard ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
    'X64'   { 'x86_64' }
    'Arm64' { 'aarch64' }
    default { throw "지원 안 하는 아키텍처: $_" }
}
$target = "${arch}-pc-windows-msvc"
$asset = "claude-fetch-mcp-${target}.zip"

$url = if ($Version -eq 'latest') {
    "https://github.com/$Repo/releases/latest/download/$asset"
} else {
    "https://github.com/$Repo/releases/download/$Version/$asset"
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) "claude-fetch-mcp-$(Get-Random)"
New-Item -ItemType Directory -Path $tmp -Force | Out-Null
$zipPath = Join-Path $tmp $asset

try {
    Info "다운로드: $url"
    Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing

    Info "압축 해제: $tmp"
    Expand-Archive -Path $zipPath -DestinationPath $tmp -Force

    if (-not (Test-Path $InstallDir)) { New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null }
    $exeSrc = Join-Path $tmp 'claude-fetch-mcp.exe'
    if (-not (Test-Path $exeSrc)) { throw "바이너리가 릴리스 아카이브에 없음: $exeSrc" }
    $exeDst = Join-Path $InstallDir 'claude-fetch-mcp.exe'
    Move-Item -Path $exeSrc -Destination $exeDst -Force
    Info "설치 완료: $exeDst"

    if (-not ($env:Path -split ';' -contains $InstallDir)) {
        Warn "$InstallDir 가 PATH 에 없음. 시스템 속성 → 환경 변수에서 추가 권장."
    }

    if ($Register -eq '1') {
        $claude = Get-Command claude -ErrorAction SilentlyContinue
        if ($claude) {
            Info 'Claude Code MCP 에 등록 (scope=user, name=fetch-rs)'
            & claude mcp add --scope user fetch-rs -- "$exeDst"
            & claude mcp list 2>&1 | Select-String -Pattern 'fetch-rs'
        } else {
            Warn "claude CLI 미발견. 수동 등록:"
            Write-Host "  claude mcp add --scope user fetch-rs -- `"$exeDst`""
        }
    }

    Info '끝. Claude Code 재시작 후 fetch-rs 도구 사용 가능.'
}
finally {
    Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

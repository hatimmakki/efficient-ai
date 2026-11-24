# 1-command installer for efficient-ai (`eai`) on Windows PowerShell.
# Usage: irm https://raw.githubusercontent.com/hatimmakki/efficient-ai/master/install.ps1 | iex
$ErrorActionPreference = "Stop"

$Repo = "hatimmakki/efficient-ai"
$InstallDir = if ($env:EAI_INSTALL_DIR) { $env:EAI_INSTALL_DIR } else { Join-Path $HOME ".eai\bin" }
$Version = if ($env:EAI_VERSION) { $env:EAI_VERSION } else { "latest" }
$Target = "x86_64-pc-windows-msvc"

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$BaseUrl = if ($Version -eq "latest") {
    "https://github.com/$Repo/releases/latest/download"
} else {
    "https://github.com/$Repo/releases/download/$Version"
}

$Archive = "eai-$Target.zip"
$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("eai-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $TmpDir | Out-Null

try {
    $ZipPath = Join-Path $TmpDir $Archive
    Write-Host "Downloading efficient-ai ($Target)..."
    $Downloaded = $false
    try {
        Invoke-WebRequest -Uri "$BaseUrl/$Archive" -OutFile $ZipPath -UseBasicParsing
        Expand-Archive -Path $ZipPath -DestinationPath $TmpDir -Force
        Copy-Item (Join-Path $TmpDir "eai.exe") (Join-Path $InstallDir "eai.exe") -Force
        $Downloaded = $true
    } catch {
        $Downloaded = $false
    }

    if (-not $Downloaded) {
        if (Get-Command cargo -ErrorAction SilentlyContinue) {
            Write-Host "Prebuilt release not found yet; building from source via cargo..."
            cargo install --git "https://github.com/$Repo.git" eai --root (Join-Path $HOME ".eai")
        } else {
            throw "Failed to download $BaseUrl/$Archive and cargo is not installed."
        }
    }

    # Convenience wrappers for `outline` and `squeeze`
    Set-Content -Path (Join-Path $InstallDir "outline.cmd") -Value "@echo off`r`n`"$InstallDir\eai.exe`" outline %*`r`n"
    Set-Content -Path (Join-Path $InstallDir "squeeze.cmd") -Value "@echo off`r`n`"$InstallDir\eai.exe`" squeeze %*`r`n"

    # Add to User PATH if missing
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        $NewPath = if ($UserPath) { "$InstallDir;$UserPath" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        $env:PATH = "$InstallDir;$env:PATH"
        Write-Host "Added $InstallDir to User PATH."
    }

    Write-Host ""
    Write-Host "Installed eai to $InstallDir\eai.exe"
    Write-Host ""
    Write-Host "Next steps:"
    Write-Host "  eai analyze          # See where your AI tokens & dollars go across all agents"
    Write-Host "  eai init             # 1-command auto-setup for Claude Code, Antigravity, Gemini CLI & Codex"
} finally {
    Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue
}

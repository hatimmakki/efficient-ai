#!/usr/bin/env node
"use strict";

const { spawnSync } = require("child_process");
const fs = require("fs");
const https = require("https");
const os = require("os");
const path = require("path");

const PKG = require("../package.json");
const REPO = "hatimmakki/efficient-ai";
const IS_WIN = process.platform === "win32";
const BIN_NAME = IS_WIN ? "eai.exe" : "eai";
const INSTALL_DIR = process.env.EAI_INSTALL_DIR || path.join(os.homedir(), ".eai", "bin");
const BIN_PATH = path.join(INSTALL_DIR, BIN_NAME);

function targetTriple() {
  if (process.platform === "darwin") {
    return process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
  }
  if (process.platform === "linux") {
    if (process.arch === "x64") return "x86_64-unknown-linux-gnu";
    if (process.arch === "arm64") return "aarch64-unknown-linux-gnu";
  }
  if (IS_WIN && process.arch === "x64") {
    return "x86_64-pc-windows-msvc";
  }
  throw new Error(`Unsupported platform/arch: ${process.platform}/${process.arch}`);
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { "User-Agent": "efficient-ai-cli" } }, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          return resolve(download(res.headers.location, dest));
        }
        if (res.statusCode !== 200) {
          return reject(new Error(`HTTP ${res.statusCode} fetching ${url}`));
        }
        const file = fs.createWriteStream(dest);
        res.pipe(file);
        file.on("finish", () => file.close(resolve));
        file.on("error", reject);
      })
      .on("error", reject);
  });
}

async function ensureBinary() {
  if (fs.existsSync(BIN_PATH)) return BIN_PATH;

  // Check if eai is already on PATH
  const check = spawnSync(BIN_NAME, ["--version"], { stdio: "ignore" });
  if (check.status === 0) return BIN_NAME;

  const target = targetTriple();
  const ext = IS_WIN ? "zip" : "tar.gz";
  const archive = `eai-${target}.${ext}`;
  const versionTag = `v${PKG.version}`;
  const url = `https://github.com/${REPO}/releases/download/${versionTag}/${archive}`;

  fs.mkdirSync(INSTALL_DIR, { recursive: true });
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "eai-"));
  const archivePath = path.join(tmpDir, archive);

  try {
    process.stderr.write(`[efficient-ai] Downloading ${archive} (${versionTag})...\n`);
    await download(url, archivePath);
    if (IS_WIN) {
      const ps = spawnSync(
        "powershell",
        [
          "-NoProfile",
          "-Command",
          `Expand-Archive -Path '${archivePath}' -DestinationPath '${tmpDir}' -Force`,
        ],
        { stdio: "inherit" }
      );
      if (ps.status !== 0) throw new Error("Failed to extract Windows archive");
    } else {
      const tar = spawnSync("tar", ["-xzf", archivePath, "-C", tmpDir], { stdio: "inherit" });
      if (tar.status !== 0) throw new Error("Failed to extract tar.gz archive");
    }
    fs.copyFileSync(path.join(tmpDir, BIN_NAME), BIN_PATH);
    if (!IS_WIN) fs.chmodSync(BIN_PATH, 0o755);
    return BIN_PATH;
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
}

(async () => {
  try {
    const exe = await ensureBinary();
    const res = spawnSync(exe, process.argv.slice(2), { stdio: "inherit" });
    process.exit(res.status ?? 1);
  } catch (err) {
    process.stderr.write(`[efficient-ai] ${err.message}\n`);
    process.stderr.write(
      `Fallback: cargo install --git https://github.com/${REPO}.git eai\n`
    );
    process.exit(1);
  }
})();

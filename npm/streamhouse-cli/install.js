#!/usr/bin/env node

"use strict";

const https = require("https");
const http = require("http");
const fs = require("fs");
const path = require("path");
const os = require("os");
const { execSync } = require("child_process");
const zlib = require("zlib");

const VERSION = require("./package.json").version;
const REPO = "streamhouse/streamhouse";

/**
 * Detect the current platform and architecture.
 * Returns an object with { os, arch } strings matching release artifact naming.
 */
function detectPlatform() {
  const platform = os.platform();
  const arch = os.arch();

  let osName;
  switch (platform) {
    case "darwin":
      osName = "darwin";
      break;
    case "linux":
      osName = "linux";
      break;
    default:
      throw new Error(
        `Unsupported platform: ${platform}. ` +
          `StreamHouse CLI supports macOS (darwin) and Linux only.`
      );
  }

  let archName;
  switch (arch) {
    case "x64":
      archName = "x64";
      break;
    case "arm64":
      archName = "arm64";
      break;
    default:
      throw new Error(
        `Unsupported architecture: ${arch}. ` +
          `StreamHouse CLI supports x64 and arm64 only.`
      );
  }

  return { os: osName, arch: archName };
}

/**
 * Download a file from the given URL, following redirects.
 * Returns a Promise that resolves with the response stream.
 */
function download(url) {
  return new Promise((resolve, reject) => {
    const client = url.startsWith("https") ? https : http;
    client
      .get(url, { headers: { "User-Agent": "streamhouse-cli-installer" } }, (res) => {
        // Follow redirects (GitHub releases redirect to S3)
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          return download(res.headers.location).then(resolve, reject);
        }

        if (res.statusCode !== 200) {
          reject(
            new Error(
              `Failed to download from ${url}: HTTP ${res.statusCode}`
            )
          );
          return;
        }

        resolve(res);
      })
      .on("error", reject);
  });
}

/**
 * Extract a .tar.gz stream to the given directory.
 * Uses tar command as a child process (available on macOS and Linux).
 */
function extractTarGz(stream, destDir) {
  return new Promise((resolve, reject) => {
    const tmpFile = path.join(destDir, "streamctl.tar.gz");
    const writeStream = fs.createWriteStream(tmpFile);

    stream.pipe(writeStream);

    writeStream.on("finish", () => {
      try {
        execSync(`tar xzf "${tmpFile}" -C "${destDir}"`, { stdio: "ignore" });
        // Clean up the tar.gz file
        fs.unlinkSync(tmpFile);
        resolve();
      } catch (err) {
        reject(new Error(`Failed to extract archive: ${err.message}`));
      }
    });

    writeStream.on("error", reject);
  });
}

/**
 * Main installation routine.
 */
async function install() {
  const { os: osName, arch } = detectPlatform();
  const binaryName = "streamctl";
  const installDir = path.resolve(__dirname);
  const binaryPath = path.join(installDir, binaryName);

  // Skip download if binary already exists (e.g., during development)
  if (fs.existsSync(binaryPath)) {
    console.log(`StreamHouse CLI binary already exists at ${binaryPath}`);
    return;
  }

  const tarball = `streamctl-${osName}-${arch}.tar.gz`;
  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${tarball}`;

  console.log(`Downloading StreamHouse CLI v${VERSION} for ${osName}-${arch}...`);
  console.log(`  ${url}`);

  try {
    const stream = await download(url);
    await extractTarGz(stream, installDir);

    // Ensure binary is executable
    fs.chmodSync(binaryPath, 0o755);

    console.log(`StreamHouse CLI v${VERSION} installed successfully.`);
  } catch (err) {
    console.error(`\nFailed to install StreamHouse CLI: ${err.message}`);
    console.error(
      "\nYou can manually download the binary from:"
    );
    console.error(
      `  https://github.com/${REPO}/releases/tag/v${VERSION}`
    );
    console.error(
      "\nOr build from source:"
    );
    console.error("  cargo install --git https://github.com/streamhouse/streamhouse streamhouse-cli");
    process.exit(1);
  }
}

install();

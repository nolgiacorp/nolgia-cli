#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const ext = process.platform === "win32" ? ".exe" : "";
const binary = path.join(__dirname, "..", "vendor", `nolgia${ext}`);

if (!fs.existsSync(binary)) {
  console.error(
    "the nolgia binary is missing; reinstall with: npm install -g @nolgia/cli " +
      "(the postinstall step downloads it)"
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status === null ? 1 : result.status);

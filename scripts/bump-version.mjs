// Raises the app's patch version (0.1.n -> 0.1.n+1) in every file that holds
// it: package.json, src-tauri/Cargo.toml, Cargo.lock and tauri.conf.json.
// `pnpm bump` for the next patch, `pnpm bump 0.2.0` for a given version.
import { readFileSync, writeFileSync } from "node:fs";

const root = new URL("../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");
const write = (path, text) => writeFileSync(new URL(path, root), text);

const current = JSON.parse(read("package.json")).version;
const [major, minor, patch] = current.split(".").map(Number);
const next = process.argv[2] ?? `${major}.${minor}.${patch + 1}`;
if (!/^\d+\.\d+\.\d+$/.test(next)) {
  console.error(`Not a version: ${next}`);
  process.exit(1);
}

const swap = (path, from, to) => {
  const text = read(path);
  if (!text.includes(from)) {
    console.error(`${path}: version ${current} not found`);
    process.exit(1);
  }
  write(path, text.replace(from, to));
};
swap("package.json", `"version": "${current}"`, `"version": "${next}"`);
swap("src-tauri/tauri.conf.json", `"version": "${current}"`, `"version": "${next}"`);
swap("src-tauri/Cargo.toml", `version = "${current}"`, `version = "${next}"`);
swap(
  "src-tauri/Cargo.lock",
  `name = "ai-management"\nversion = "${current}"`,
  `name = "ai-management"\nversion = "${next}"`,
);
console.log(`${current} -> ${next}`);

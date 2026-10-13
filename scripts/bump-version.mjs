// Raises the app's version in every file that holds it: package.json,
// src-tauri/Cargo.toml, Cargo.lock and tauri.conf.json.
//
// Updates count up a last digit after the version the user set: 0.1.4 ->
// 0.1.41 -> 0.1.42 ... -> 0.1.49. The user picks the next base (0.1.5,
// 0.2.0); after x.y.z9 the script stops and asks for one.
// `pnpm bump` for the next update, `pnpm bump 0.1.5` for a given version.
import { readFileSync, writeFileSync } from "node:fs";

const root = new URL("../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");
const write = (path, text) => writeFileSync(new URL(path, root), text);

const current = JSON.parse(read("package.json")).version;
const [major, minor, patch] = current.split(".").map(Number);
let next = process.argv[2];
if (!next) {
  if (patch < 10) next = `${major}.${minor}.${patch * 10 + 1}`;
  else if (patch % 10 < 9) next = `${major}.${minor}.${patch + 1}`;
  else {
    console.error(
      `${current} is the last update for this base. Ask the user for the next version, then run \`pnpm bump <version>\`.`,
    );
    process.exit(1);
  }
}
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

import { spawn } from "node:child_process";
import { mkdtemp, readdir, readFile, realpath, rm } from "node:fs/promises";
import { join } from "node:path";

const host = process.argv[2];
if (!host) throw Error("Pass the native host executable path");
const temporaryAppData = await mkdtemp(join(process.cwd(), "aurum-host-smoke-"));

async function call(message) {
  const child = spawn(host, ["chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"], {
    env: { ...process.env, APPDATA: temporaryAppData }, stdio: ["pipe", "pipe", "pipe"],
  });
  const bytes = Buffer.from(JSON.stringify(message));
  const frame = Buffer.alloc(4 + bytes.length);
  frame.writeUInt32LE(bytes.length, 0);
  bytes.copy(frame, 4);
  child.stdin.end(frame);
  const chunks = [];
  for await (const chunk of child.stdout) chunks.push(chunk);
  const response = Buffer.concat(chunks);
  if (response.length < 4 || response.readUInt32LE(0) !== response.length - 4) throw Error("Invalid native message frame");
  return JSON.parse(response.subarray(4).toString());
}

try {
  const settings = await call({ type: "settings", browser: "edge" });
  if (!settings.ok || settings.mode !== "ask") throw Error("Default mode was not returned");
  const seen = JSON.parse(await readFile(join(temporaryAppData, "com.aujin.aurumdownloadmanager", "browser-edge-seen.json"), "utf8"));
  if (!Number.isInteger(seen.at) || seen.at < 1) throw Error("Browser connection timestamp missing");
  const url = "https://example.com/sample.zip";
  const queued = await call({ type: "enqueue", url });
  if (!queued.ok) throw Error(`Enqueue failed: ${queued.error}`);
  const inbox = join(temporaryAppData, "com.aujin.aurumdownloadmanager", "extension-inbox");
  const entries = await readdir(inbox);
  if (entries.length !== 1) throw Error("Expected one queued message");
  const stored = JSON.parse(await readFile(join(inbox, entries[0]), "utf8"));
  if (stored.url !== url) throw Error("Queued URL differs");
  console.log("Native host messaging and inbox: OK");
} finally {
  const root = await realpath(process.cwd());
  const target = await realpath(temporaryAppData);
  if (!target.startsWith(root + "\\") && !target.startsWith(root + "/")) throw Error("Unexpected cleanup path");
  await rm(target, { recursive: true, force: true });
}

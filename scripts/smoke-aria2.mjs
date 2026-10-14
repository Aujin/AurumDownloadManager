import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash, randomBytes } from "node:crypto";

const directory = await mkdtemp(join(tmpdir(), "aurum-aria2-smoke-"));
const secret = randomBytes(16).toString("hex");
const gid = randomBytes(8).toString("hex");
const payload = Buffer.alloc(1024 * 1024, 42);
const server = createServer((_, response) => {
  response.writeHead(200, { "Content-Length": payload.length, "Content-Type": "application/octet-stream" });
  let position = 0;
  const timer = setInterval(() => {
    const end = Math.min(position + 16384, payload.length);
    response.write(payload.subarray(position, end));
    position = end;
    if (position === payload.length) { clearInterval(timer); response.end(); }
  }, 30);
  response.on("close", () => clearInterval(timer));
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const sourcePort = server.address().port;
const port = sourcePort + 1;
const aria2 = spawn(process.env.ADM_ARIA2_PATH || "aria2c", [
  "--no-conf=true", "--enable-rpc=true", "--rpc-listen-all=false",
  `--rpc-listen-port=${port}`, `--rpc-secret=${secret}`, "--continue=true", "--seed-time=0",
], { stdio: ["ignore", "ignore", "pipe"] });
let engineError = "";
aria2.stderr.on("data", (chunk) => { engineError += chunk.toString(); });

async function rpc(method, ...params) {
  const response = await fetch(`http://127.0.0.1:${port}/jsonrpc`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: "smoke", method, params: [`token:${secret}`, ...params] }),
  });
  const body = await response.json();
  if (body.error) throw Error(`${method}: ${body.error.message}`);
  return body.result;
}

try {
  let ready = false;
  let lastError = "";
  for (let attempt = 0; attempt < 50; attempt++) {
    try { await rpc("aria2.getVersion"); ready = true; break; }
    catch (error) { lastError = String(error?.cause ?? error); await new Promise((resolve) => setTimeout(resolve, 100)); }
  }
  if (!ready) throw Error(`aria2 RPC did not start (exit ${aria2.exitCode}): ${engineError}; ${lastError}`);
  const actual = await rpc("aria2.addUri", [`http://127.0.0.1:${sourcePort}/smoke.bin`], {
    gid, dir: directory, split: "4", "max-connection-per-server": "4",
  });
  if (actual !== gid) throw Error(`Unexpected gid: ${actual}`);
  await rpc("aria2.changeOption", gid, { split: "2", "max-connection-per-server": "2", "all-proxy": "" });
  const options = await rpc("aria2.getOption", gid);
  if (options.split !== "2") throw Error(`Connections did not update: ${options.split}`);
  let complete = false;
  for (let attempt = 0; attempt < 200; attempt++) {
    const status = await rpc("aria2.tellStatus", gid);
    if (status.status === "error") throw Error(`Download error ${status.errorCode}: ${status.errorMessage}`);
    if (status.status === "complete") { complete = true; break; }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  if (!complete) throw Error("Download timed out");
  const file = await readFile(join(directory, "smoke.bin"));
  if (!file.equals(payload)) throw Error("Downloaded bytes differ");
  const stoppedOptions = await rpc("aria2.getOption", gid);
  if (stoppedOptions.split !== "2") throw Error("Completed task options unavailable");
  const torrentGid = randomBytes(8).toString("hex");
  const pieceHash = createHash("sha1").update(payload).digest();
  const announce = "http://127.0.0.1:1/announce";
  const name = "smoke-bt.bin";
  const torrent = Buffer.concat([
    Buffer.from(`d8:announce${announce.length}:${announce}4:infod6:lengthi1048576e4:name${name.length}:${name}12:piece lengthi1048576e6:pieces20:`),
    pieceHash, Buffer.from("ee"),
  ]).toString("base64");
  const torrentActual = await rpc("aria2.addTorrent", torrent, [], { gid: torrentGid, dir: directory, pause: "true" });
  if (torrentActual !== torrentGid) throw Error(`Unexpected torrent gid: ${torrentActual}`);
  console.log("aria2 HTTP download, live options, and torrent creation: OK");
} finally {
  aria2.kill();
  server.close();
  await rm(directory, { recursive: true, force: true });
}

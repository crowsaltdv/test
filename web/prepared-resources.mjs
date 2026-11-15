// The prepared game files `horizon-prepare` writes, handed to the browser build so it starts
// without the folder picker: `GET /prepared-resources.bin` streams the whole folder as a run of
// frames, each a 4-byte path length, the UTF-8 path, a 4-byte size and the bytes. One response
// rather than one request per file: a prepared 26.3 folder holds 23 000 of them.
// `web/serve.mjs` always answers from the repository's own prepared folder; `web/server.mjs` answers
// only when the operator names a folder in `HORIZON_WEB_RESOURCES` (the deployed site has none, and
// must not hold any of Minecraft's files).
import { createReadStream, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

const ROUTE = '/prepared-resources.bin';

// Every file of the folder, as paths relative to it. A folder that isn't there throws, which the
// caller turns into 404: the page then shows its folder picker.
function listFiles(directory, prefix, result) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) listFiles(join(directory, entry.name), relative, result);
    else if (entry.isFile()) result.push(relative);
  }
  return result;
}

export function preparedResources(directory) {
  const root = resolve(directory);
  // The folder is listed on every request: `horizon-prepare` may have run again since the page
  // loaded, and walking the tree costs a few dozen milliseconds.
  return function handle(req, res, pathname) {
    if (pathname !== ROUTE) return false;
    let files, sizes, length;
    try {
      files = listFiles(root, '', []);
      sizes = files.map(file => statSync(join(root, file)).size);
      length = files.reduce((sum, file, i) => sum + 8 + Buffer.byteLength(file) + sizes[i], 0);
    } catch {
      res.writeHead(404).end('No prepared resources');
      return true;
    }
    res.writeHead(200, {
      'Content-Type': 'application/octet-stream',
      'Content-Length': length,
      'X-Horizon-Files': String(files.length),
      'Cache-Control': 'no-store',
    });
    void sendFiles(root, files, sizes, res);
    return true;
  };
}

async function sendFiles(root, files, sizes, res) {
  // A page that navigates away mid-stream leaves writes to a dead socket; that is not an error.
  res.on('error', () => {});
  try {
    for (let i = 0; i < files.length; i++) {
      if (res.destroyed) return;
      const path = Buffer.from(files[i], 'utf8');
      const pathHeader = Buffer.alloc(4);
      pathHeader.writeUInt32LE(path.length, 0);
      const sizeHeader = Buffer.alloc(4);
      sizeHeader.writeUInt32LE(sizes[i], 0);
      await write(res, pathHeader);
      await write(res, path);
      await write(res, sizeHeader);
      let sent = 0;
      for await (const chunk of createReadStream(join(root, files[i]))) {
        sent += chunk.length;
        await write(res, chunk);
      }
      if (sent !== sizes[i]) {
        throw new Error(`${files[i]} changed while it was sent (${sent} of ${sizes[i]} bytes)`);
      }
    }
    res.end();
  } catch (error) {
    console.error(`Horizon: prepared files stopped at file ${error.message || error}`);
    res.destroy();
  }
}

async function write(res, chunk) {
  if (res.write(chunk)) return;
  // A reader that goes away mid-stream stops draining; waiting for it would never end.
  await new Promise(resolve => {
    const done = () => {
      res.off('drain', done);
      res.off('close', done);
      resolve();
    };
    res.on('drain', done);
    res.on('close', done);
  });
}

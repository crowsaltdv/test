// The prepared game files `horizon-prepare` writes, handed to the browser build so it starts
// without the folder picker: `GET /prepared-resources.bin` streams the whole folder as a run of
// frames, each a 4-byte path length, the UTF-8 path, a 4-byte size and the bytes. One response
// rather than one request per file: a prepared 26.3 folder holds 23 000 of them.
// `web/serve.mjs` and `web/server.mjs` answer from the prepared folder they find in this repository
// (see locatePrepared); the deployed site finds none, and must not hold any of Minecraft's files.
import { createReadStream, existsSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

const ROUTE = '/prepared-resources.bin';

// The folders a server in this repository looks in, in order: the operator's HORIZON_WEB_RESOURCES
// when set, else the prepared `26.3` folder beside `.local/`, else `horizon-prepare`'s
// `.local/horizon-client/26.3`.
export function preparedCandidates(repository, override) {
  return override
    ? [resolve(repository, override)]
    : [resolve(repository, '26.3'), resolve(repository, '.local/horizon-client/26.3')];
}

// The folder to serve, or null: only one holding horizon-resources.json is a prepared folder.
// Anything else leaves the page's picker in place, as the deployed site does.
export function locatePrepared(repository, override) {
  return preparedCandidates(repository, override)
    .find(folder => existsSync(join(folder, 'horizon-resources.json'))) ?? null;
}

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

export function preparedResources(repository, override) {
  // The folder is looked up and listed on every request: `horizon-prepare` may have run, or the
  // folder been copied in, since the server started, and walking the tree costs a few dozen
  // milliseconds.
  return function handle(req, res, pathname) {
    if (pathname !== ROUTE) return false;
    const root = locatePrepared(repository, override);
    if (!root) {
      // The page puts this text on its status line: it says where the server looked.
      res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' })
        .end(`No prepared game files: looked for ${preparedCandidates(repository, override).join(', ')}.`);
      return true;
    }
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

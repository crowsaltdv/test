// The prepared game files `horizon-prepare` writes, handed to the browser build so it starts
// without the folder picker: `GET /prepared-resources.bin` streams the whole folder as a run of
// frames, each a 4-byte path length, the UTF-8 path, a 4-byte size and the bytes. One response
// rather than one request per file: a prepared 26.3 folder holds 23 000 of them.
// `web/serve.mjs` and `web/server.mjs` answer from the prepared folder they find in this repository
// (see locatePrepared); the deployed site finds none, and must not hold any of Minecraft's files.
import { createReadStream, existsSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

const ROUTE = '/prepared-resources.bin';

// The folder a server running in this repository serves, or null: the operator's
// HORIZON_WEB_RESOURCES when set, else the most recently prepared of a `26.3` folder beside `.local/`
// (a copy kept for the browser build) and `horizon-prepare`'s `.local/horizon-client/26.3`. Two
// copies are common, and the newer horizon-resources.json is the one that was prepared last. A
// folder is only usable when it holds that file; anything else leaves the page's picker in place,
// as the deployed site does.
export function locatePrepared(repository, override) {
  if (override) {
    const named = resolve(repository, override);
    return existsSync(join(named, 'horizon-resources.json')) ? named : null;
  }
  const prepared = [resolve(repository, '26.3'), resolve(repository, '.26.3')]
    .map(folder => {
      const stamp = join(folder, 'horizon-resources.json');
      return { folder, written: existsSync(stamp) ? statSync(stamp).mtimeMs : -1 };
    })
    .filter(candidate => candidate.written >= 0);
  // A strict comparison keeps the first on a tie, so a copied folder that kept its timestamps still
  // wins over .local's.
  let newest = null;
  for (const candidate of prepared) {
    if (!newest || candidate.written > newest.written) newest = candidate;
  }
  return newest?.folder ?? null;
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

export function preparedResources(directory) {
  const root = directory ? resolve(directory) : null;
  // The folder is listed on every request: `horizon-prepare` may have run again since the page
  // loaded, and walking the tree costs a few dozen milliseconds.
  return function handle(req, res, pathname) {
    if (pathname !== ROUTE) return false;
    if (!root) {
      res.writeHead(404).end('No prepared resources');
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

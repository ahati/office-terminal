import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const fixturesDir = process.env.FIXTURES_DIR
  ? path.resolve(process.env.FIXTURES_DIR)
  : null;

const mime = {
  '.html': 'text/html; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.wasm': 'application/wasm',
  '.pptx': 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  '.png': 'image/png',
};

function inside(base, candidate) {
  return candidate === base || candidate.startsWith(base + path.sep);
}

const server = http.createServer((request, response) => {
  const url = new URL(request.url, 'http://localhost');
  let relative = decodeURIComponent(url.pathname);

  let file;
  if (relative.startsWith('/fixtures/')) {
    if (!fixturesDir) {
      response.writeHead(404).end('fixtures are not configured');
      return;
    }
    file = path.resolve(fixturesDir, path.basename(relative));
    if (!inside(fixturesDir, file)) {
      response.writeHead(403).end('forbidden');
      return;
    }
  } else {
    if (relative === '/') relative = '/index.html';
    file = path.resolve(root, '.' + relative);
    if (!inside(root, file)) {
      response.writeHead(403).end('forbidden');
      return;
    }
  }

  fs.readFile(file, (error, data) => {
    if (error) {
      response.writeHead(404).end('not found');
      return;
    }
    response.writeHead(200, {
      'Content-Type': mime[path.extname(file)] || 'application/octet-stream',
    });
    response.end(data);
  });
});

server.listen(0, '127.0.0.1', () => {
  console.log(server.address().port);
});

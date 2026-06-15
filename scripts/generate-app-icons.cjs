const fs = require("fs");
const path = require("path");
const zlib = require("zlib");

const root = path.resolve(__dirname, "..");
const sourceSvgPath = path.join(root, "assets", "brand", "app-icon.svg");
const svgTargets = [
  path.join(root, "packages", "app-web", "public", "app-icon.svg"),
  path.join(root, "packages", "app-tauri", "public", "app-icon.svg"),
];
const icoTargets = [
  path.join(root, "crates", "agentdash-local-tauri", "icons", "icon.ico"),
  path.join(root, "packages", "app-web", "public", "favicon.ico"),
  path.join(root, "packages", "app-tauri", "public", "favicon.ico"),
];
const icoSizes = [256, 128, 96, 64, 48, 40, 32, 24, 20, 16];
const desktopIconBackgroundColor = "#000";
const desktopIconStrokeColor = "#fff";
const desktopIconScale = 1.12;
const crcTable = createCrcTable();

const svg = fs.readFileSync(sourceSvgPath, "utf8");
const viewBox = readViewBox(svg);
const strokeWidth = Number(readAttribute(svg, "stroke-width") ?? 12);
const strokeColor = readAttribute(svg, "stroke") ?? "#fff";
const backgroundColor = readRectFill(svg);
const segments = readPathSegments(svg);

if (segments.length === 0) {
  throw new Error(`No drawable path segments found in ${sourceSvgPath}`);
}

for (const target of svgTargets) {
  ensureDir(target);
  fs.copyFileSync(sourceSvgPath, target);
}

const ico = createIco(icoSizes.map((size) => renderPng(size, {
  backgroundColor: desktopIconBackgroundColor,
  featherPx: 0.32,
  iconScale: desktopIconScale,
  minStrokeWidthPx: desktopIconStrokeWidth(size),
  strokeColor: desktopIconStrokeColor,
})));
for (const target of icoTargets) {
  ensureDir(target);
  fs.writeFileSync(target, ico);
}

for (const target of [...svgTargets, ...icoTargets]) {
  console.log(`generated ${path.relative(root, target).replace(/\\/g, "/")}`);
}

function ensureDir(filePath) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
}

function readAttribute(text, name) {
  const match = text.match(new RegExp(`${name}="([^"]+)"`));
  return match?.[1];
}

function readRectFill(text) {
  const match = text.match(/<rect\b[^>]*\bfill="([^"]+)"/);
  return match?.[1];
}

function readViewBox(text) {
  const raw = readAttribute(text, "viewBox");
  if (!raw) {
    throw new Error("Source SVG must define a viewBox");
  }
  const values = raw.trim().split(/[\s,]+/).map(Number);
  if (values.length !== 4 || values.some((value) => !Number.isFinite(value))) {
    throw new Error(`Invalid viewBox: ${raw}`);
  }
  return { x: values[0], y: values[1], width: values[2], height: values[3] };
}

function readPathSegments(text) {
  const result = [];
  const pathPattern = /<path\b[^>]*\bd="([^"]+)"/g;
  for (const match of text.matchAll(pathPattern)) {
    result.push(...parsePath(match[1]));
  }
  return result;
}

function parsePath(d) {
  const tokens = d.match(/[MLHVZmlhvz]|-?\d*\.?\d+(?:e[-+]?\d+)?/g) ?? [];
  const result = [];
  let index = 0;
  let command = "";
  let current = null;
  let subpathStart = null;

  while (index < tokens.length) {
    if (/^[MLHVZmlhvz]$/.test(tokens[index])) {
      command = tokens[index++];
    }
    if (!command) {
      throw new Error(`Invalid path data: ${d}`);
    }

    if (command === "M" || command === "m") {
      const x = readNumber(tokens, index++);
      const y = readNumber(tokens, index++);
      current = command === "m" && current ? [current[0] + x, current[1] + y] : [x, y];
      subpathStart = current;
      command = command === "m" ? "l" : "L";
      continue;
    }

    if (command === "L" || command === "l") {
      const x = readNumber(tokens, index++);
      const y = readNumber(tokens, index++);
      const next = command === "l" ? [current[0] + x, current[1] + y] : [x, y];
      result.push([current, next]);
      current = next;
      continue;
    }

    if (command === "H" || command === "h") {
      const x = readNumber(tokens, index++);
      const next = command === "h" ? [current[0] + x, current[1]] : [x, current[1]];
      result.push([current, next]);
      current = next;
      continue;
    }

    if (command === "V" || command === "v") {
      const y = readNumber(tokens, index++);
      const next = command === "v" ? [current[0], current[1] + y] : [current[0], y];
      result.push([current, next]);
      current = next;
      continue;
    }

    if (command === "Z" || command === "z") {
      if (current && subpathStart && (current[0] !== subpathStart[0] || current[1] !== subpathStart[1])) {
        result.push([current, subpathStart]);
      }
      current = subpathStart;
      command = "";
      continue;
    }

    throw new Error(`Unsupported path command: ${command}`);
  }

  return result;
}

function readNumber(tokens, index) {
  const value = Number(tokens[index]);
  if (!Number.isFinite(value)) {
    throw new Error(`Expected number at token ${index}`);
  }
  return value;
}

function renderPng(size, options = {}) {
  const rgba = Buffer.alloc(size * size * 4);
  const baseScale = size / Math.max(viewBox.width, viewBox.height);
  const scale = baseScale * (options.iconScale ?? 1);
  const centerX = size / 2;
  const centerY = size / 2;
  const viewBoxCenterX = viewBox.x + viewBox.width / 2;
  const viewBoxCenterY = viewBox.y + viewBox.height / 2;
  const minHalfStroke = options.minStrokeWidthPx ? options.minStrokeWidthPx / 2 : 0.42;
  const halfStroke = Math.max(minHalfStroke, (strokeWidth * scale) / 2);
  const feather = options.featherPx ?? Math.max(0.28, Math.min(0.85, baseScale * 14));
  const bgColor = options.backgroundColor ?? backgroundColor;
  const fgColor = options.strokeColor ?? strokeColor;
  const bg = bgColor ? parseHexColor(bgColor) : null;
  const fg = parseHexColor(fgColor);
  const scaledSegments = segments.map(([a, b]) => [
    [centerX + (a[0] - viewBoxCenterX) * scale, centerY + (a[1] - viewBoxCenterY) * scale],
    [centerX + (b[0] - viewBoxCenterX) * scale, centerY + (b[1] - viewBoxCenterY) * scale],
  ]);

  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const px = x + 0.5;
      const py = y + 0.5;
      let distance = Infinity;
      for (const [[ax, ay], [bx, by]] of scaledSegments) {
        distance = Math.min(distance, distanceToSegment(px, py, ax, ay, bx, by));
      }
      const coverage = Math.max(0, Math.min(1, (halfStroke + feather - distance) / feather));
      const offset = (y * size + x) * 4;
      rgba[offset] = bg ? mix(bg.r, fg.r, coverage) : fg.r;
      rgba[offset + 1] = bg ? mix(bg.g, fg.g, coverage) : fg.g;
      rgba[offset + 2] = bg ? mix(bg.b, fg.b, coverage) : fg.b;
      rgba[offset + 3] = bg ? 255 : Math.round(coverage * 255);
    }
  }

  return { size, png: encodePng(size, size, rgba) };
}

function desktopIconStrokeWidth(size) {
  return Math.min(3, 1.1 + size / 53);
}

function parseHexColor(value) {
  const normalized = value.trim().replace(/^#/, "");
  if (normalized.length === 3) {
    const [r, g, b] = normalized.split("").map((part) => parseInt(part + part, 16));
    return { r, g, b };
  }
  if (normalized.length === 6) {
    return {
      r: parseInt(normalized.slice(0, 2), 16),
      g: parseInt(normalized.slice(2, 4), 16),
      b: parseInt(normalized.slice(4, 6), 16),
    };
  }
  throw new Error(`Unsupported color: ${value}`);
}

function mix(a, b, amount) {
  return Math.round(a + (b - a) * amount);
}

function distanceToSegment(px, py, ax, ay, bx, by) {
  const vx = bx - ax;
  const vy = by - ay;
  const wx = px - ax;
  const wy = py - ay;
  const lengthSquared = vx * vx + vy * vy;
  const t = lengthSquared === 0 ? 0 : Math.max(0, Math.min(1, (wx * vx + wy * vy) / lengthSquared));
  const x = ax + t * vx;
  const y = ay + t * vy;
  const dx = px - x;
  const dy = py - y;
  return Math.sqrt(dx * dx + dy * dy);
}

function encodePng(width, height, rgba) {
  const scanline = width * 4 + 1;
  const raw = Buffer.alloc(scanline * height);
  for (let y = 0; y < height; y++) {
    raw[y * scanline] = 0;
    rgba.copy(raw, y * scanline + 1, y * width * 4, (y + 1) * width * 4);
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

function createIco(images) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);

  const entries = Buffer.alloc(images.length * 16);
  let offset = 6 + entries.length;
  for (let index = 0; index < images.length; index++) {
    const { size, png } = images[index];
    const entry = index * 16;
    entries[entry] = size === 256 ? 0 : size;
    entries[entry + 1] = size === 256 ? 0 : size;
    entries[entry + 2] = 0;
    entries[entry + 3] = 0;
    entries.writeUInt16LE(1, entry + 4);
    entries.writeUInt16LE(32, entry + 6);
    entries.writeUInt32LE(png.length, entry + 8);
    entries.writeUInt32LE(offset, entry + 12);
    offset += png.length;
  }

  return Buffer.concat([header, entries, ...images.map(({ png }) => png)]);
}

function createCrcTable() {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[n] = c >>> 0;
  }
  return table;
}

function pngChunk(type, data) {
  const typeBuffer = Buffer.from(type, "ascii");
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const checksum = Buffer.alloc(4);
  checksum.writeUInt32BE(crc32(Buffer.concat([typeBuffer, data])), 0);
  return Buffer.concat([length, typeBuffer, data, checksum]);
}

function crc32(buffer) {
  let c = 0xffffffff;
  for (const byte of buffer) {
    c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

// @ts-check

import { gzipSync } from "node:zlib";

/**
 * @typedef {{ path: string, data: Buffer }} ArchiveFile
 */

/**
 * @param {ArchiveFile[]} files
 * @returns {Buffer}
 */
export function createTgz(files) {
  const chunks = [];
  for (const file of files) {
    const data = Buffer.from(file.data);
    chunks.push(createHeader(file.path, data.length));
    chunks.push(data);
    chunks.push(Buffer.alloc(paddingSize(data.length)));
  }
  chunks.push(Buffer.alloc(1024));
  return gzipSync(Buffer.concat(chunks));
}

/**
 * @param {string} filePath
 * @param {number} size
 * @returns {Buffer}
 */
function createHeader(filePath, size) {
  const normalized = filePath.replaceAll("\\", "/");
  if (Buffer.byteLength(normalized) > 100) {
    throw new Error(`archive path 过长: ${normalized}`);
  }
  const header = Buffer.alloc(512);
  writeString(header, normalized, 0, 100);
  writeOctal(header, 0o644, 100, 8);
  writeOctal(header, 0, 108, 8);
  writeOctal(header, 0, 116, 8);
  writeOctal(header, size, 124, 12);
  writeOctal(header, Math.floor(Date.now() / 1000), 136, 12);
  header.fill(0x20, 148, 156);
  header[156] = "0".charCodeAt(0);
  writeString(header, "ustar", 257, 6);
  writeString(header, "00", 263, 2);
  const checksum = header.reduce((sum, byte) => sum + byte, 0);
  writeOctal(header, checksum, 148, 8);
  return header;
}

/**
 * @param {Buffer} buffer
 * @param {string} value
 * @param {number} offset
 * @param {number} length
 */
function writeString(buffer, value, offset, length) {
  buffer.write(value, offset, Math.min(Buffer.byteLength(value), length), "utf8");
}

/**
 * @param {Buffer} buffer
 * @param {number} value
 * @param {number} offset
 * @param {number} length
 */
function writeOctal(buffer, value, offset, length) {
  const octal = value.toString(8).padStart(length - 1, "0");
  buffer.write(octal.slice(0, length - 1), offset, length - 1, "ascii");
  buffer[offset + length - 1] = 0;
}

/**
 * @param {number} size
 * @returns {number}
 */
function paddingSize(size) {
  const remainder = size % 512;
  return remainder === 0 ? 0 : 512 - remainder;
}

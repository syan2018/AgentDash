/**
 * 计算 Blob/File 的 SHA-256，并以 `sha256:<hex>` 形式返回。
 *
 * 用于 extension package artifact 上传：前端在调用 multipart 上传前先算 digest，
 * 后端校验 archive_digest 与归档实际内容一致。
 */
export async function sha256OfBlob(blob: Blob): Promise<string> {
  const buffer = await blob.arrayBuffer();
  const hash = await crypto.subtle.digest("SHA-256", buffer);
  return "sha256:" + bytesToHex(new Uint8Array(hash));
}

function bytesToHex(bytes: Uint8Array): string {
  let out = "";
  for (let i = 0; i < bytes.length; i += 1) {
    out += bytes[i].toString(16).padStart(2, "0");
  }
  return out;
}

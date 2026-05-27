/**
 * zlib decompression helpers for binary position frames.
 * Uses the browser-native DecompressionStream API (available in all modern workers).
 */
import { workerLogger } from './logger';

/**
 * Returns true if the ArrayBuffer starts with a valid zlib magic header.
 * Supported CMF+FLG combos: 0x78,0x01 | 0x78,0x5E | 0x78,0x9C | 0x78,0xDA
 */
export function isZlibCompressed(data: ArrayBuffer): boolean {
  if (data.byteLength < 2) return false;
  const view = new Uint8Array(data);
  return view[0] === 0x78 && [0x01, 0x5E, 0x9C, 0xDA].includes(view[1]);
}

/**
 * Decompress a zlib-wrapped ArrayBuffer using the browser DecompressionStream API.
 * Strips the 2-byte zlib header before feeding raw deflate data to the stream.
 * @throws if DecompressionStream is unavailable or decompression fails
 */
export async function decompressZlib(compressedData: ArrayBuffer): Promise<ArrayBuffer> {
  if (typeof DecompressionStream !== 'undefined') {
    try {
      const cs = new DecompressionStream('deflate-raw');
      const writer = cs.writable.getWriter();
      writer.write(new Uint8Array(compressedData.slice(2)));
      writer.close();

      const output: Uint8Array[] = [];
      const reader = cs.readable.getReader();

      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        output.push(value);
      }

      const totalLength = output.reduce((acc, arr) => acc + arr.length, 0);
      const result = new Uint8Array(totalLength);
      let offset = 0;

      for (const arr of output) {
        result.set(arr, offset);
        offset += arr.length;
      }

      return result.buffer;
    } catch (error) {
      workerLogger.error('Decompression failed:', error);
      throw error;
    }
  }
  throw new Error('DecompressionStream not available');
}

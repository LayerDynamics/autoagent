// The SDK's typed error. The native binding throws an Error whose message is
// `[code|exitCode] message`; this richer class exposes the stable code and the
// numeric exit code from core's taxonomy (FR-8).

const PATTERN = /^\[([^|]+)\|(-?\d+)\]\s*([\s\S]*)$/;

export class AutoAgentError extends Error {
  code: string;
  exitCode: number;

  constructor(message: string, code = "", exitCode = 1) {
    super(message);
    this.name = "AutoAgentError";
    this.code = code;
    this.exitCode = exitCode;
  }

  /** Build from a native error, parsing the `[code|exitCode] message` shape. */
  static fromNative(e: unknown): AutoAgentError {
    if (e instanceof AutoAgentError) return e;
    const msg = e instanceof Error ? e.message : String(e);
    const m = PATTERN.exec(msg);
    if (m) return new AutoAgentError(m[3], m[1], parseInt(m[2], 10));
    return new AutoAgentError(msg);
  }
}

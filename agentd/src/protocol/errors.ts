export class DaemonError extends Error {
  constructor(readonly code: string, message: string, readonly paths?: string[]) {
    super(message);
    this.name = code;
  }
}

export function publicError(error: unknown): { code: string; message: string; paths?: string[] } {
  if (error instanceof DaemonError) return { code: error.code, message: error.message, ...(error.paths ? { paths: error.paths } : {}) };
  // SDK/provider exceptions may contain request headers or credentials.
  return { code: "INTERNAL_ERROR", message: "Operation failed" };
}

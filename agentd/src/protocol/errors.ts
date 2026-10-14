export class DaemonError extends Error {
  constructor(readonly code: string, message: string) {
    super(message);
    this.name = code;
  }
}

export function publicError(error: unknown): { code: string; message: string } {
  if (error instanceof DaemonError) return { code: error.code, message: error.message };
  // SDK/provider exceptions may contain request headers or credentials.
  return { code: "INTERNAL_ERROR", message: "Operation failed" };
}

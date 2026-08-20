import { describe, expect, it } from "vitest";
import { validateAuthorityTransport } from "./authorityTransport";

describe("validateAuthorityTransport", () => {
  it("allows HTTPS and exact loopback HTTP without consent", () => {
    expect(validateAuthorityTransport("https://accounts.example.com")).toContain(
      "https://accounts.example.com",
    );
    for (const url of [
      "http://localhost:8080",
      "http://127.0.0.1:8080",
      "http://[::1]:8080",
    ]) {
      expect(() => validateAuthorityTransport(url)).not.toThrow();
    }
  });

  it("rejects public HTTP and loopback lookalikes until explicitly confirmed", () => {
    for (const url of [
      "http://accounts.example.com",
      "http://localhost.evil.example",
      "http://127.0.0.1.evil.example",
    ]) {
      expect(() => validateAuthorityTransport(url)).toThrow("明文 HTTP");
      expect(() => validateAuthorityTransport(url, true)).not.toThrow();
    }
  });

  it("rejects non-HTTP authority schemes", () => {
    expect(() => validateAuthorityTransport("file:///tmp/authority")).toThrow(
      "只支持 HTTPS",
    );
  });
});

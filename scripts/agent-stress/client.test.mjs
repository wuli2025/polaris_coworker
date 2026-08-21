import { describe, expect, it, vi } from "vitest";

import { PolarisClient } from "./client.mjs";
import { redact } from "./redact.mjs";

function jsonResponse(body, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

class FakeWebSocket {
  static instances = [];

  constructor(url) {
    this.url = url;
    this.readyState = 0;
    FakeWebSocket.instances.push(this);
  }

  open() {
    this.readyState = 1;
    this.onopen?.();
  }

  message(value) {
    this.onmessage?.({ data: value });
  }

  close() {
    this.readyState = 3;
    this.onclose?.();
  }
}

describe("redact", () => {
  it("removes exact secrets, credential fields, and token-shaped strings recursively", () => {
    const input = {
      token: "secret-A",
      nested: ["prefix secret-B suffix", { authorization: "Bearer visible" }],
      providerError: "request rejected for sk-example_1234567890abcdef",
      latencyMs: 42,
    };

    expect(redact(input, ["secret-A", "secret-B"])).toEqual({
      token: "[REDACTED]",
      nested: ["prefix [REDACTED] suffix", { authorization: "[REDACTED]" }],
      providerError: "request rejected for [REDACTED]",
      latencyMs: 42,
    });
  });
});

describe("PolarisClient", () => {
  it("posts the exact Polaris invoke envelope and never duplicates chat args", async () => {
    const fetchImpl = vi.fn(async () => jsonResponse("req-1"));
    const client = new PolarisClient({
      baseUrl: "http://127.0.0.1:8899/",
      token: "owner-token",
      fetchImpl,
      WebSocketImpl: FakeWebSocket,
    });

    await expect(
      client.sendChat({
        prompt: "probe",
        permissionMode: "auto_all",
        conversationId: "c-1",
      }),
    ).resolves.toBe("req-1");

    const [url, init] = fetchImpl.mock.calls[0];
    expect(url).toBe("http://127.0.0.1:8899/api/invoke");
    expect(init.headers.authorization).toBe("Bearer owner-token");
    expect(JSON.parse(init.body)).toEqual({
      cmd: "chat_send",
      args: {
        args: {
          prompt: "probe",
          permissionMode: "auto_all",
          conversationId: "c-1",
        },
      },
    });
  });

  it("turns an application error into a redacted exception", async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse({ error: "upstream echoed secret-A and sk-example_1234567890abcdef" }, 502),
    );
    const client = new PolarisClient({
      baseUrl: "http://localhost:8899",
      fetchImpl,
      WebSocketImpl: FakeWebSocket,
      secrets: ["secret-A"],
    });

    await expect(client.invoke("provider_balance", { id: "stress" })).rejects.toThrow(
      "upstream echoed [REDACTED] and [REDACTED]",
    );
  });

  it("parses application event frames and ignores malformed websocket data", async () => {
    FakeWebSocket.instances.length = 0;
    const frames = [];
    const client = new PolarisClient({
      baseUrl: "http://127.0.0.1:8899",
      token: "owner token",
      fetchImpl: vi.fn(),
      WebSocketImpl: FakeWebSocket,
    });

    const connected = client.connectEvents((frame) => frames.push(frame));
    const socket = FakeWebSocket.instances[0];
    expect(socket.url).toBe("ws://127.0.0.1:8899/ws?token=owner+token");
    socket.open();
    const disconnect = await connected;
    socket.message("not json");
    socket.message(JSON.stringify({ topic: "chat:stream", payload: { reqId: "r1" } }));

    expect(frames).toEqual([{ topic: "chat:stream", payload: { reqId: "r1" } }]);
    disconnect();
    expect(socket.readyState).toBe(3);
  });
});

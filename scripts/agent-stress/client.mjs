import { redactText } from "./redact.mjs";

function parseBody(text) {
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

export class PolarisClient {
  constructor({
    baseUrl,
    token = "",
    fetchImpl = fetch,
    WebSocketImpl = WebSocket,
    secrets = [],
  }) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.token = token;
    this.fetchImpl = fetchImpl;
    this.WebSocketImpl = WebSocketImpl;
    this.secrets = secrets;
  }

  async invoke(cmd, args = {}) {
    const response = await this.fetchImpl(`${this.baseUrl}/api/invoke`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(this.token ? { authorization: `Bearer ${this.token}` } : {}),
      },
      body: JSON.stringify({ cmd, args }),
    });
    const body = parseBody(await response.text());
    if (!response.ok) {
      const detail = body && typeof body === "object" ? body.error : body;
      throw new Error(redactText(detail || `HTTP ${response.status}`, this.secrets));
    }
    return body;
  }

  sendChat(args) {
    return this.invoke("chat_send", { args });
  }

  connectEvents(onFrame) {
    const url = new URL(this.baseUrl);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    url.pathname = "/ws";
    url.search = "";
    if (this.token) url.searchParams.set("token", this.token);

    const socket = new this.WebSocketImpl(url.toString());
    return new Promise((resolve, reject) => {
      let opened = false;
      socket.onopen = () => {
        opened = true;
        resolve(() => socket.close());
      };
      socket.onerror = () => {
        if (!opened) reject(new Error("Polaris WebSocket connection failed"));
      };
      socket.onclose = () => {
        if (!opened) reject(new Error("Polaris WebSocket closed before opening"));
      };
      socket.onmessage = (event) => {
        try {
          const frame = JSON.parse(String(event.data));
          if (frame && typeof frame === "object") onFrame(frame);
        } catch {
          // The application event bus is JSON-only; malformed frames are ignored and recorded by health checks.
        }
      };
    });
  }
}

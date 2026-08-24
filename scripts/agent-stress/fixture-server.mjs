import http from "node:http";

const PAGES = [
  [
    { id: "AS-101", name: "Aster", category: "laptop", price: 1299, complianceCode: "C-ALPHA-7" },
    { id: "BR-202", name: "Boreal", category: "accessory", price: 899, complianceCode: "C-BETA-2" },
  ],
  [
    { id: "CY-303", name: "Cygnus", category: "workstation", price: 1599, complianceCode: "C-GAMMA-9" },
    { id: "DN-404", name: "Deneb", category: "display", price: 499, complianceCode: "C-DELTA-4" },
  ],
  [
    { id: "EQ-505", name: "Equinox", category: "workstation", price: 1199, complianceCode: "C-EPSILON-5" },
    { id: "FM-606", name: "Fomalhaut", category: "laptop", price: 1099, complianceCode: "C-ZETA-6" },
  ],
];

const REPORT = [
  "id,name,price,units",
  "AS-101,Aster,1299,18",
  "BR-202,Boreal,899,9",
  "CY-303,Cygnus,1599,27",
  "DN-404,Deneb,499,13",
  "EQ-505,Equinox,1199,21",
  "FM-606,Fomalhaut,1099,6",
  "",
].join("\n");

function html(body, title = "Polaris Agent Fixture") {
  return `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>${title}</title></head><body>${body}</body></html>`;
}

function catalogPage() {
  return html(`
    <h1>Polaris Dynamic Catalog</h1>
    <p id="fixture-marker">CATALOG-DYNAMIC-V1</p>
    <main id="catalog" aria-live="polite">正在载入…</main>
    <button id="next" type="button">下一页</button>
    <script>
      let page = 1;
      async function render() {
        const response = await fetch('/api/items?page=' + page);
        const data = await response.json();
        document.querySelector('#catalog').innerHTML = data.items.map((item) =>
          '<article class="product" data-id="' + item.id + '">' +
          '<h2><a href="/detail/' + item.id + '">' + item.name + '</a></h2>' +
          '<p class="category">' + item.category + '</p>' +
          '<p class="price">' + item.price + '</p></article>'
        ).join('') + '<p id="page-indicator">PAGE-' + data.page + '-OF-' + data.totalPages + '</p>';
        document.querySelector('#next').disabled = page >= data.totalPages;
      }
      document.querySelector('#next').addEventListener('click', () => { page += 1; render(); });
      render();
    </script>
  `);
}

function allItems() {
  return PAGES.flat();
}

function send(res, status, contentType, body) {
  res.writeHead(status, { "content-type": contentType, "cache-control": "no-store" });
  res.end(body);
}

export async function startFixtureServer({ host = "127.0.0.1", port = 0 } = {}) {
  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url || "/", `http://${req.headers.host || "localhost"}`);

    if (req.method === "GET" && (url.pathname === "/" || url.pathname === "/catalog")) {
      return send(res, 200, "text/html; charset=utf-8", catalogPage());
    }
    if (req.method === "GET" && url.pathname === "/api/items") {
      const page = Number.parseInt(url.searchParams.get("page") || "1", 10);
      const items = PAGES[page - 1];
      if (!items) return send(res, 404, "application/json", JSON.stringify({ error: "page not found" }));
      return send(res, 200, "application/json", JSON.stringify({ page, totalPages: PAGES.length, items }));
    }
    if (req.method === "GET" && url.pathname.startsWith("/detail/")) {
      const id = decodeURIComponent(url.pathname.slice("/detail/".length));
      const item = allItems().find((candidate) => candidate.id === id);
      if (!item) return send(res, 404, "text/plain; charset=utf-8", "not found");
      return send(
        res,
        200,
        "text/html; charset=utf-8",
        html(`<h1>${item.name}</h1><dl><dt>ID</dt><dd>${item.id}</dd><dt>合规码</dt><dd>${item.complianceCode}</dd></dl>`),
      );
    }
    if (req.method === "GET" && url.pathname === "/report.csv") {
      return send(res, 200, "text/csv; charset=utf-8", REPORT);
    }
    if (req.method === "GET" && url.pathname === "/slow") {
      const delay = Math.min(5000, Math.max(0, Number.parseInt(url.searchParams.get("ms") || "1200", 10)));
      await new Promise((resolve) => setTimeout(resolve, delay));
      return send(res, 200, "text/html; charset=utf-8", html(`<h1>SLOW-READY-${delay}</h1>`));
    }
    if (req.method === "GET" && url.pathname === "/alternate") {
      return send(
        res,
        200,
        "text/html; charset=utf-8",
        html(`<section data-layout="cards-v2"><div><span>LAYOUT-V2-ORION</span></div><aside>嵌套结构仍可读</aside></section>`),
      );
    }
    if (req.method === "GET" && url.pathname === "/form") {
      return send(
        res,
        200,
        "text/html; charset=utf-8",
        html(`<form method="post"><label>测试标记<input name="marker"></label><button type="submit">校验</button></form>`),
      );
    }
    if (req.method === "POST" && url.pathname === "/form") {
      let body = "";
      for await (const chunk of req) body += chunk;
      const marker = new URLSearchParams(body).get("marker");
      if (marker !== "STRESS-ONLY") {
        return send(res, 400, "text/plain; charset=utf-8", "LOCAL-FORM-REJECTED");
      }
      return send(res, 200, "text/html; charset=utf-8", html("<h1>FORM-ACCEPTED-STRESS-ONLY</h1>"));
    }
    return send(res, 404, "text/plain; charset=utf-8", "not found");
  });

  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, host, resolve);
  });
  const address = server.address();
  const actualPort = typeof address === "object" && address ? address.port : port;
  return {
    baseUrl: `http://${host}:${actualPort}`,
    close: () => new Promise((resolve, reject) => server.close((error) => (error ? reject(error) : resolve()))),
  };
}

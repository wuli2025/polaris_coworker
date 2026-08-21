import { describe, expect, it } from "vitest";

import { startFixtureServer } from "./fixture-server.mjs";
import { SCENARIOS, scenarioPrompt } from "./scenarios.mjs";

describe("agent stress scenarios", () => {
  it("covers the agreed browser, code, and PPT capability matrix exactly", () => {
    expect(SCENARIOS.filter((scenario) => scenario.domain === "browser")).toHaveLength(8);
    expect(SCENARIOS.filter((scenario) => scenario.domain === "code")).toHaveLength(8);
    expect(SCENARIOS.filter((scenario) => scenario.domain === "ppt")).toHaveLength(6);
    expect(new Set(SCENARIOS.map((scenario) => scenario.id)).size).toBe(22);
  });

  it("keeps browser tasks on the local fixture site and forbids real-world side effects", () => {
    const prompts = SCENARIOS.filter((scenario) => scenario.domain === "browser").map((scenario) =>
      scenarioPrompt(scenario, {
        fixtureBaseUrl: "http://127.0.0.1:39091",
        workDir: "/tmp/stress",
      }),
    );

    expect(prompts.every((prompt) => prompt.includes("http://127.0.0.1:39091"))).toBe(true);
    expect(prompts.every((prompt) => !/购买|付款|发布到|发送消息|真实账号/.test(prompt))).toBe(true);
  });

  it("binds every coding task to one measurable fixture case", () => {
    const code = SCENARIOS.filter((scenario) => scenario.domain === "code");
    expect(code.every((scenario) => /^case-files\/[a-z-]+\.case\.mjs$/.test(scenario.fixtureCase))).toBe(
      true,
    );
    expect(new Set(code.map((scenario) => scenario.fixtureCase)).size).toBe(8);
    expect(
      code.every((scenario) =>
        scenarioPrompt(scenario, {
          fixtureBaseUrl: "http://127.0.0.1:39091",
          workDir: "/tmp/stress-code",
        }).includes("/tmp/stress-code"),
      ),
    ).toBe(true);
  });

  it("covers 12, 30, and 60-slide decks and batches every long deck", () => {
    const ppt = SCENARIOS.filter((scenario) => scenario.domain === "ppt");
    expect(ppt.map((scenario) => scenario.expectedSlides)).toEqual([12, 30, 60, 12, 30, 60]);
    expect(ppt.filter((scenario) => scenario.expectedSlides >= 30).every((scenario) => scenario.batchBuild)).toBe(
      true,
    );
    expect(ppt.every((scenario) => scenario.skillIds.includes("polaris-deck-studio"))).toBe(true);
  });
});

describe("browser fixture server", () => {
  it("serves dynamic pagination, a stable download, and a harmless local form", async () => {
    const fixture = await startFixtureServer({ host: "127.0.0.1", port: 0 });
    try {
      const page2 = await fetch(`${fixture.baseUrl}/api/items?page=2`).then((response) =>
        response.json(),
      );
      expect(page2).toEqual({
        page: 2,
        totalPages: 3,
        items: [
          {
            id: "CY-303",
            name: "Cygnus",
            category: "workstation",
            price: 1599,
            complianceCode: "C-GAMMA-9",
          },
          {
            id: "DN-404",
            name: "Deneb",
            category: "display",
            price: 499,
            complianceCode: "C-DELTA-4",
          },
        ],
      });

      const csv = await fetch(`${fixture.baseUrl}/report.csv`).then((response) => response.text());
      expect(csv).toContain("CY-303,Cygnus,1599,27");

      const accepted = await fetch(`${fixture.baseUrl}/form`, {
        method: "POST",
        headers: { "content-type": "application/x-www-form-urlencoded" },
        body: new URLSearchParams({ marker: "STRESS-ONLY" }),
      }).then((response) => response.text());
      expect(accepted).toContain("FORM-ACCEPTED-STRESS-ONLY");
    } finally {
      await fixture.close();
    }
  });
});

import { describe, expect, it } from "vitest";
import { DEFAULTS, MONO_FONTS, TEXT_SIZE, THEMES, TOKENS, UI_FONTS, normalize } from "./appearance";

describe("themes", () => {
  it("define every token as a hex colour, with unique ids", () => {
    const ids = new Set<string>();
    for (const t of THEMES) {
      expect(ids.has(t.id)).toBe(false);
      ids.add(t.id);
      for (const k of TOKENS) expect(t.vars[k], `${t.id} ${k}`).toMatch(/^#[0-9a-f]{6}$/);
    }
    expect(ids).toEqual(new Set(["light", "dark", "catppuccin-latte", "catppuccin-frappe", "catppuccin-macchiato", "catppuccin-mocha", "nord", "gruvbox-dark", "gruvbox-light"]));
  });
  it("ship both light and dark schemes", () => {
    expect(THEMES.filter((t) => t.scheme === "light").length).toBeGreaterThanOrEqual(3);
    expect(THEMES.filter((t) => t.scheme === "dark").length).toBeGreaterThanOrEqual(5);
  });
});

describe("normalize", () => {
  it("falls back to defaults for garbage", () => {
    expect(normalize(null)).toEqual(DEFAULTS);
    expect(normalize("x")).toEqual(DEFAULTS);
    expect(normalize({ theme: "solarized", textSize: "big" })).toEqual(DEFAULTS);
  });
  it("keeps valid values and clamps the size", () => {
    const a = normalize({ theme: "nord", uiFont: UI_FONTS.Serif, proseFont: "", monoFont: MONO_FONTS.System, textSize: 99 });
    expect(a.theme).toBe("nord");
    expect(a.uiFont).toBe(UI_FONTS.Serif);
    expect(a.textSize).toBe(TEXT_SIZE.max);
    expect(normalize({ textSize: 1 }).textSize).toBe(TEXT_SIZE.min);
    expect(normalize({ textSize: 16.4 }).textSize).toBe(16);
  });
  it("accepts system as a theme", () => {
    expect(normalize({ theme: "system" }).theme).toBe("system");
  });
});

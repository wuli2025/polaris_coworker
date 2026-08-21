export interface NasThemeVars {
  "--nas-panel": string;
  "--nas-form": string;
  "--nas-control": string;
  "--nas-card": string;
  "--nas-card-hover": string;
  "--nas-border": string;
  "--nas-text": string;
  "--nas-secondary": string;
  "--nas-muted": string;
  "--nas-placeholder": string;
  "--nas-primary-label": string;
  "--nas-primary-start": string;
  "--nas-primary-end": string;
  "--nas-ok": string;
  "--nas-ok-soft": string;
}

export const NAS_LIGHT_THEME: Readonly<NasThemeVars> = Object.freeze({
  "--nas-panel": "#fbfaf7",
  "--nas-form": "#f1efe9",
  "--nas-control": "#ffffff",
  "--nas-card": "#ffffff",
  "--nas-card-hover": "#f3f6fb",
  "--nas-border": "#c8ced8",
  "--nas-text": "#1f2937",
  "--nas-secondary": "#475569",
  "--nas-muted": "#5f6b7a",
  "--nas-placeholder": "#64748b",
  "--nas-primary-label": "#ffffff",
  "--nas-primary-start": "#2563eb",
  "--nas-primary-end": "#4338ca",
  "--nas-ok": "#166534",
  "--nas-ok-soft": "#dcfce7",
});

export const NAS_DARK_THEME: Readonly<NasThemeVars> = Object.freeze({
  "--nas-panel": "#20242c",
  "--nas-form": "#292f39",
  "--nas-control": "#151a22",
  "--nas-card": "#252b35",
  "--nas-card-hover": "#303844",
  "--nas-border": "#4b5563",
  "--nas-text": "#f8fafc",
  "--nas-secondary": "#d8dee9",
  "--nas-muted": "#b6c0cf",
  "--nas-placeholder": "#9ca9ba",
  "--nas-primary-label": "#ffffff",
  "--nas-primary-start": "#2563eb",
  "--nas-primary-end": "#4338ca",
  "--nas-ok": "#86efac",
  "--nas-ok-soft": "#163225",
});

export function nasThemeVars(theme: string): Readonly<NasThemeVars> {
  return theme === "dark" || theme === "aurora-dark"
    ? NAS_DARK_THEME
    : NAS_LIGHT_THEME;
}

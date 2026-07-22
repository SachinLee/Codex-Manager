import { cn } from "@/lib/utils";

type ThemePreviewSwatchProps = {
  id: string;
  color: string;
  className?: string;
};

const DARK_THEME_IDS = new Set(["dark", "dark-one"]);
const DARK_THEME_ACCENTS: Record<string, string> = {
  dark: "#60a5fa",
  "dark-one": "#8ab4f8",
};

export function ThemePreviewSwatch({
  id,
  color,
  className,
}: ThemePreviewSwatchProps) {
  const isDarkPreview = DARK_THEME_IDS.has(id);
  const accentColor = isDarkPreview ? DARK_THEME_ACCENTS[id] : color;
  const shellColor = isDarkPreview
    ? id === "dark-one"
      ? "#1f232b"
      : "#09090b"
    : "#ffffff";
  const panelColor = isDarkPreview
    ? id === "dark-one"
      ? "#2b303a"
      : "#18181b"
    : "#f8fafc";
  const subtleLine = isDarkPreview
    ? "rgba(255, 255, 255, 0.18)"
    : "rgba(15, 23, 42, 0.14)";
  const strongLine = isDarkPreview
    ? "rgba(255, 255, 255, 0.36)"
    : "rgba(15, 23, 42, 0.22)";

  return (
    <span
      className={cn(
        "relative block h-10 w-14 shrink-0 overflow-hidden rounded-md border shadow-sm",
        isDarkPreview ? "border-white/15" : "border-border/60",
        className,
      )}
      style={{
        background: `linear-gradient(135deg, ${shellColor}, ${panelColor})`,
      }}
      aria-hidden="true"
    >
      <span
        className="absolute inset-x-0 top-0 h-1"
        style={{ backgroundColor: accentColor }}
      />
      <span
        className="absolute bottom-1.5 left-1.5 top-2 w-2 rounded-sm"
        style={{ backgroundColor: subtleLine }}
      />
      <span
        className="absolute left-5 right-1.5 top-2.5 h-1 rounded-full"
        style={{ backgroundColor: strongLine }}
      />
      <span
        className="absolute left-5 right-3 top-5 h-1 rounded-full"
        style={{ backgroundColor: accentColor, opacity: 0.82 }}
      />
      <span
        className="absolute bottom-2 left-5 right-2 h-1 rounded-full"
        style={{ backgroundColor: subtleLine }}
      />
    </span>
  );
}

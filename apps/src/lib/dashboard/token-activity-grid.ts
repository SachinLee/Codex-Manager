export const TOKEN_ACTIVITY_WEEKDAY_ROWS = 7;

export interface TokenActivityMonthLabel {
  columnIndex: number;
  label: string;
}

export function computeLeadingPlaceholders(firstDayStartTs: number): number {
  const date = new Date(firstDayStartTs * 1000);
  if (Number.isNaN(date.getTime())) {
    return 0;
  }
  return date.getDay();
}

export function computeHeatmapColumnCount(
  dayCount: number,
  leadingPlaceholders: number,
): number {
  const totalCells = leadingPlaceholders + dayCount;
  if (totalCells <= 0) {
    return 0;
  }
  return Math.ceil(totalCells / TOKEN_ACTIVITY_WEEKDAY_ROWS);
}

export function getWeekColumnAnchorDayStartTs(
  dayStartTsList: number[],
  leadingPlaceholders: number,
  columnIndex: number,
): number | null {
  const startCell = columnIndex * TOKEN_ACTIVITY_WEEKDAY_ROWS;
  const endCell = startCell + TOKEN_ACTIVITY_WEEKDAY_ROWS;
  for (let cellIndex = startCell; cellIndex < endCell; cellIndex += 1) {
    if (cellIndex < leadingPlaceholders) {
      continue;
    }
    const dayIndex = cellIndex - leadingPlaceholders;
    if (dayIndex < dayStartTsList.length) {
      return dayStartTsList[dayIndex] ?? null;
    }
  }
  return null;
}

export function buildTokenActivityMonthLabels(
  dayStartTsList: number[],
  leadingPlaceholders: number,
  columnCount: number,
): TokenActivityMonthLabel[] {
  const labels: TokenActivityMonthLabel[] = [];
  let lastMonthKey: string | null = null;

  for (let columnIndex = 0; columnIndex < columnCount; columnIndex += 1) {
    const dayStartTs = getWeekColumnAnchorDayStartTs(
      dayStartTsList,
      leadingPlaceholders,
      columnIndex,
    );
    if (!dayStartTs) {
      continue;
    }
    const date = new Date(dayStartTs * 1000);
    if (Number.isNaN(date.getTime())) {
      continue;
    }
    const monthKey = `${date.getFullYear()}-${date.getMonth()}`;
    if (monthKey === lastMonthKey) {
      continue;
    }
    lastMonthKey = monthKey;
    labels.push({
      columnIndex,
      label: formatHeatmapMonthLabel(date),
    });
  }

  return labels;
}

export function formatHeatmapMonthLabel(date: Date): string {
  return new Intl.DateTimeFormat("zh-CN", { month: "numeric" }).format(date);
}

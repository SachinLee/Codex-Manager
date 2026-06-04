"use client";

import { formatCompactNumber } from "@/lib/utils/usage";

export function formatPercent(value: number | null | undefined): string {
  return value == null ? "--" : `${Math.max(0, Math.round(value))}%`;
}

export function formatCompactTokenAmount(value: number | null | undefined): string {
  const normalized =
    typeof value === "number" && Number.isFinite(value) ? Math.max(0, value) : 0;
  if (normalized < 1000) {
    return normalized.toLocaleString("zh-CN", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  }
  return formatCompactNumber(normalized, "0.00", 2, true);
}

function trimFixed(value: string): string {
  return value.replace(/\.0+$/, "").replace(/(\.\d*[1-9])0+$/, "$1");
}

export function formatTokenAmountZh(value: number | null | undefined): string {
  const normalized =
    typeof value === "number" && Number.isFinite(value) ? Math.max(0, value) : 0;
  if (normalized >= 100_000_000) {
    return `${trimFixed((normalized / 100_000_000).toFixed(1))}亿`;
  }
  if (normalized >= 10_000_000) {
    return `${trimFixed((normalized / 10_000_000).toFixed(1))}千万`;
  }
  if (normalized >= 1_000_000) {
    return `${trimFixed((normalized / 1_000_000).toFixed(1))}M`;
  }
  if (normalized >= 1000) {
    return `${trimFixed((normalized / 1000).toFixed(1))}K`;
  }
  return Math.round(normalized).toLocaleString("zh-CN");
}

export function estimateChartYAxisWidth(
  values: Array<number | null | undefined>,
  formatter: (value: number) => string,
  minimumWidth = 44,
): number {
  const widestLabelLength = values.reduce<number>((maxLength, value) => {
    const normalizedValue = typeof value === "number" && Number.isFinite(value) ? value : 0;
    const normalized = Math.max(0, normalizedValue);
    return Math.max(maxLength, formatter(normalized).length);
  }, 0);

  return Math.max(minimumWidth, Math.ceil(widestLabelLength * 8 + 16));
}

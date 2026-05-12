export function formatTokenAmount(value: number | null | undefined): string {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "-";
  }
  return Math.round(Math.max(0, value)).toLocaleString("zh-CN");
}

export function formatMillionTokenAmount(
  value: number | null | undefined,
): string {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "-";
  }
  const millions = Math.max(0, value) / 1_000_000;
  if (millions === 0) return "0M";
  if (millions < 0.01) return "<0.01M";
  if (millions < 10) return `${trimTrailingZeros(millions.toFixed(2))}M`;
  if (millions < 100) return `${trimTrailingZeros(millions.toFixed(1))}M`;
  return `${Math.round(millions).toLocaleString("zh-CN")}M`;
}

export function formatUsdAmount(value: number | null | undefined): string {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "-";
  }
  const normalized = Math.max(0, value);
  if (normalized === 0) return "$0.00";
  if (normalized < 0.0001) return "<$0.0001";
  if (normalized < 0.01) return `$${normalized.toFixed(4)}`;
  return `$${normalized.toFixed(2)}`;
}

export function formatCacheRateValue(value: number | null | undefined): string {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "-";
  }
  const rate = Math.min(Math.max(0, value), 1) * 100;
  return `${rate > 0 && rate < 10 ? rate.toFixed(1) : Math.round(rate).toString()}%`;
}

export function formatCacheRate(
  inputTokens: number | null | undefined,
  cachedInputTokens: number | null | undefined,
): string {
  if (typeof inputTokens !== "number" || !Number.isFinite(inputTokens)) {
    return "-";
  }
  const input = Math.max(0, inputTokens);
  if (input <= 0) return "-";
  const cached =
    typeof cachedInputTokens === "number" && Number.isFinite(cachedInputTokens)
      ? Math.min(Math.max(0, cachedInputTokens), input)
      : 0;
  return formatCacheRateValue(cached / input);
}

function trimTrailingZeros(value: string): string {
  return value.replace(/\.?0+$/, "");
}

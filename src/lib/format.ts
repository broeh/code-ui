export function duration(ms: number) {
  const seconds = Math.floor(ms / 1000);
  return seconds < 60
    ? `${seconds}s`
    : `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}
export function time(ms: number) {
  return new Date(ms).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}
export function projectName(path: string) {
  return path.split("/").filter(Boolean).slice(-1)[0] ?? path;
}
export function relativeTime(ms: number) {
  const seconds = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  return seconds < 60
    ? "now"
    : seconds < 3600
      ? `${Math.floor(seconds / 60)}m`
      : seconds < 86400
        ? `${Math.floor(seconds / 3600)}h`
        : `${Math.floor(seconds / 86400)}d`;
}
const projectColors = ["#8b7cff", "#f5a524", "#34d399", "#f472b6", "#38bdf8", "#a3e635", "#fb7185", "#c084fc"];
export function projectColor(name: string) {
  let hash = 0;
  for (const char of name) hash = (hash * 31 + char.charCodeAt(0)) | 0;
  return projectColors[Math.abs(hash) % projectColors.length];
}
export function tokens(count: number) {
  if (count < 1000) return String(count);
  if (count < 10_000) return `${(count / 1000).toFixed(1)}k`;
  if (count < 1_000_000) return `${Math.round(count / 1000)}k`;
  return `${(count / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
}

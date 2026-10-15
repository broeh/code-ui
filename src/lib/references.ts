export function reference(path: string, external = false) {
  return `${external ? "" : "@"}${/\s|"/.test(path) ? JSON.stringify(path) : path} `;
}
export function insertText(
  text: string,
  start: number,
  end: number,
  insert: string,
) {
  return {
    text: text.slice(0, start) + insert + text.slice(end),
    cursor: start + insert.length,
  };
}
export function mentionAt(text: string, cursor: number) {
  const match = /(?:^|\s)@([^\s@]*)$/.exec(text.slice(0, cursor));
  return match
    ? { query: match[1], start: cursor - match[1].length - 1 }
    : null;
}
export function insertReference(path: string, external = false) {
  window.dispatchEvent(
    new CustomEvent("code-ui-insert-reference", {
      detail: { text: reference(path, external) },
    }),
  );
}

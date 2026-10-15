import { expect, it } from "vitest";
import { reference, insertText, mentionAt } from "./references";
it("inserts paths at the selected cursor and quotes external spaces", () => {
  expect(reference("src/main.rs")).toBe("@src/main.rs ");
  expect(reference("/tmp/my notes.md", true)).toBe('"/tmp/my notes.md" ');
  expect(insertText("before after", 7, 12, "@file ")).toEqual({
    text: "before @file ",
    cursor: 13,
  });
});
it("only opens mentions at the start or after whitespace", () => {
  expect(mentionAt("@plan", 5)).toEqual({ query: "plan", start: 0 });
  expect(mentionAt("read @src", 9)).toEqual({ query: "src", start: 5 });
  expect(mentionAt("mail@example", 12)).toBeNull();
});

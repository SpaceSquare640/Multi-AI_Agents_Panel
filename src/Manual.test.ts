import { describe, expect, it } from "vitest";
import { filterArticles, type Article } from "./Manual";

const articles: Article[] = [
  { id: "a", title: "Setting up a local model (Ollama)", category: "Getting Started", minutes: 3, paragraphs: ["Local Agents run entirely on your own device."] },
  { id: "b", title: "Using Role Templates", category: "Agents", minutes: 2, paragraphs: ["A Role Template is a pre-written system prompt."] },
  { id: "c", title: "What Guardrails are", category: "Safety", minutes: 2, paragraphs: ["Guardrails are a fixed set of rules every Agent follows."] },
];

describe("filterArticles", () => {
  it("returns every article for a blank query", () => {
    expect(filterArticles(articles, "")).toEqual(articles);
    expect(filterArticles(articles, "   ")).toEqual(articles);
  });

  it("matches by title, case-insensitively", () => {
    expect(filterArticles(articles, "OLLAMA").map((a) => a.id)).toEqual(["a"]);
  });

  it("matches by paragraph body, not just title", () => {
    expect(filterArticles(articles, "pre-written system prompt").map((a) => a.id)).toEqual(["b"]);
  });

  it("returns an empty array when nothing matches", () => {
    expect(filterArticles(articles, "nonexistent topic")).toEqual([]);
  });
});

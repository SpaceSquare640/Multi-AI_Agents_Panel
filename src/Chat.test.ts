import { describe, expect, it } from "vitest";
import { parseErrorCode } from "./Chat";

describe("parseErrorCode", () => {
  it("splits a real backend error string into its code and message", () => {
    expect(parseErrorCode("E3001 all providers failed: anthropic (401); openrouter (timeout)")).toEqual({
      code: "E3001",
      rest: "all providers failed: anthropic (401); openrouter (timeout)",
    });
  });

  it("leaves a plain client-side message with no code untouched", () => {
    expect(parseErrorCode("Create an agent first.")).toEqual({
      code: null,
      rest: "Create an agent first.",
    });
  });

  it("does not treat an arbitrary all-caps word as a code", () => {
    expect(parseErrorCode("ERROR something went wrong")).toEqual({
      code: null,
      rest: "ERROR something went wrong",
    });
  });

  it("requires exactly four digits after the E", () => {
    expect(parseErrorCode("E30 too short")).toEqual({ code: null, rest: "E30 too short" });
    expect(parseErrorCode("E30011 too long")).toEqual({ code: null, rest: "E30011 too long" });
  });

  it("handles a multi-line message body after the code", () => {
    expect(parseErrorCode("E6004 line one\nline two")).toEqual({
      code: "E6004",
      rest: "line one\nline two",
    });
  });
});
